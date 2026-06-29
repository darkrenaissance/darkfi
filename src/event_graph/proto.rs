/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! P2P protocol handlers for the Event Graph.
//!
//! Each peer connection spawns a [`ProtocolEventGraph`] instance that
//! manages message subscriptions and handles incoming events, sync
//! requests, and bidirectional range queries.

use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    slice,
    str::FromStr,
    sync::{
        atomic::{AtomicUsize, Ordering::SeqCst},
        Arc,
    },
};

use darkfi_sdk::{crypto::poseidon_hash, pasta::pallas};
use darkfi_serial::{
    async_trait, deserialize_async_partial, serialize_async, FutAsyncWriteExt, SerialDecodable,
    SerialEncodable,
};
use smol::Executor;
use tracing::{error, warn};

use super::{
    event::Header,
    rln::{self, create_slash_proof, sss_recover, RLNNode, SlashBlob},
    Event, EventGraphPtr, LayerUTips, NULL_ID, NULL_PARENTS,
};
use crate::{
    impl_p2p_message,
    net::{
        metering::{MeteringConfiguration, DEFAULT_METERING_CONFIGURATION},
        ChannelPtr, Message, MessageSubscription, ProtocolBase, ProtocolBasePtr,
        ProtocolJobsManager, ProtocolJobsManagerPtr,
    },
    system::msleep,
    util::time::NanoTimestamp,
    Error, Result,
};

/// After this many malicious-looking messages from a single peer, we
/// drop the connection.
const MALICIOUS_THRESHOLD: usize = 5;

/// If a peer sends more than this many unique events within
/// [`WINDOW_EXPIRY_TIME`], we ban them.
const WINDOW_MAXSIZE: usize = 200;

/// Rolling window length for the flood-detection counter.
const WINDOW_EXPIRY_TIME: NanoTimestamp = NanoTimestamp::from_secs(60);

/// Rolling window length for the outbound broadcast rate limiter.
const RATELIMIT_EXPIRY_TIME: NanoTimestamp = NanoTimestamp::from_secs(10);
/// Rate limiter activates above this many broadcasts in the window.
const RATELIMIT_MIN_COUNT: usize = 6;
/// Reference point for computing sleep time: when count = this value...
const RATELIMIT_SAMPLE_IDX: usize = 10;
/// Sleep this many milliseconds before broadcasting.
const RATELIMIT_SAMPLE_SLEEP: usize = 1000;

/// Maximum number of recursive round-trips when fetching missing
/// parent events from a peer during `handle_event_put`.
///
/// # Why this limit exists
///
/// When we receive a new event whose parents we don't have, we ask
/// the sender for them. Those parents may themselves reference unknown
/// grandparents, so we ask again, and so on. A malicious peer can exploit
/// this by fabricating an arbitrarily deep chain, forcing us into an
/// unbounded loop of network requests.
///
/// # What happens when the limit is hit
///
/// The event (and its unresolvable ancestry) is dropped, and the
/// peer's malicious counter is incremented. This is safe because:
///
/// * **Legitimate DAGs** rarely reach this depth. With 5 parents
///   per event and concurrent users, cross-references keep the
///   effective depth well below 1000.
/// * **The header-sync path** (`dag_sync`) is unaffected - it
///   fetches all headers in bulk by layer, with no recursion.
///   A node that's 1000+ layers behind should be using `dag_sync`
///   rather than relying on `EventPut` catch-up.
/// * **After a full sync**, subsequent `EventPut` events will
///   typically reference parents that are already known, so the
///   depth stays near 1.
const MAX_PARENT_FETCH_DEPTH: usize = 1000;

/// Capacity of the bounded broadcast channel. When the channel is
/// full, new relay events are dropped rather than blocking the
/// event processing loop - this provides backpressure and prevents
/// unbounded memory growth under sustained load.
const BROADCASTER_CAPACITY: usize = 256;

/// Maximum event bodies a peer may request in one `EventReq`.
pub const MAX_EVENT_REQ_IDS: usize = 128;
/// Maximum event bodies accepted in one `EventRep`.
pub const MAX_EVENT_REP_EVENTS: usize = MAX_EVENT_REQ_IDS;
/// Maximum tips a peer may include in one `HeaderReq`.
pub const MAX_HEADER_REQ_TIPS: usize = 1024;
/// Maximum headers returned in one `HeaderRep`.
pub const MAX_HEADER_REP_HEADERS: usize = 4096;
/// Maximum tips returned in one `TipRep`.
pub const MAX_TIP_REP_TIPS: usize = 1024;
/// Maximum events served for one paginated range request.
pub const MAX_RANGE_PAGE_SIZE: usize = 100;

pub(crate) fn count_layer_tips(tips: &LayerUTips) -> usize {
    tips.values().map(HashSet::len).sum()
}

pub(crate) fn cap_layer_tips(tips: &LayerUTips, limit: usize) -> LayerUTips {
    let mut out = BTreeMap::new();
    let mut remaining = limit;

    for (layer, hashes) in tips {
        if remaining == 0 {
            break
        }

        let mut sorted: Vec<_> = hashes.iter().copied().collect();
        sorted.sort_unstable_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        let take = sorted.len().min(remaining);
        if take > 0 {
            out.insert(*layer, sorted.into_iter().take(take).collect());
            remaining -= take;
        }
    }

    out
}

struct MovingWindow {
    times: VecDeque<NanoTimestamp>,
    expiry_time: NanoTimestamp,
}

impl MovingWindow {
    fn new(expiry: NanoTimestamp) -> Self {
        Self { times: VecDeque::new(), expiry_time: expiry }
    }

    fn clean(&mut self) {
        while let Some(ts) = self.times.front() {
            match ts.elapsed() {
                Ok(elapsed) if elapsed >= self.expiry_time => {
                    self.times.pop_front();
                }
                Err(_) => {
                    self.times.pop_front();
                } // future timestamp - remove
                _ => break,
            }
        }
    }

    fn ticktock(&mut self) {
        self.clean();
        self.times.push_back(NanoTimestamp::current_time());
    }

    fn count(&self) -> usize {
        self.times.len()
    }
}

/// Broadcast a new event (header + content + optional RLN blob).
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct EventPut(pub Event, pub Vec<u8>);
impl_p2p_message!(EventPut, "EventGraph::EventPut", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Broadcast a static-DAG event (RLN registration / slashing).
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct StaticPut(pub Event, pub Vec<u8>);
impl_p2p_message!(StaticPut, "EventGraph::StaticPut", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Request full events by their IDs.
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct EventReq(pub Vec<blake3::Hash>);
impl_p2p_message!(EventReq, "EventGraph::EventReq", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Reply with full events, plus optional aligned blobs.
///
/// `events` and `blobs` are aligned by index: `blobs[i]` is the
/// original RLN blob for `events[i]`. For non-genesis events,
/// peers MUST supply a non-empty blob - the recipient's
/// `dag_insert_with_blobs` rejects events without one. An empty
/// blob is acceptable only for genesis-shaped events.
///
/// `blobs.len() != events.len()` is wire-compatible: missing
/// trailing entries are treated as empty, which on non-genesis
/// events means rejection.
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct EventRep(pub Vec<Event>, pub Vec<Vec<u8>>);
impl_p2p_message!(EventRep, "EventGraph::EventRep", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Broadcast a single header (unused in current flow, reserved).
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct HeaderPut(pub Header);
impl_p2p_message!(HeaderPut, "EventGraph::HeaderPut", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Request headers that the peer has but we don't, given our tips.
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct HeaderReq(pub String, pub LayerUTips);
impl_p2p_message!(HeaderReq, "EventGraph::HeaderReq", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Reply with headers.
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct HeaderRep(pub Vec<Header>);
impl_p2p_message!(HeaderRep, "EventGraph::HeaderRep", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Request a peer's current unreferenced tips for a DAG.
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct TipReq(pub String);
impl_p2p_message!(TipReq, "EventGraph::TipReq", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Reply with unreferenced tips.
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct TipRep(pub LayerUTips);
impl_p2p_message!(TipRep, "EventGraph::TipRep", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Pagination direction for [`RangeReq`].
#[derive(Clone, Debug, SerialEncodable, SerialDecodable)]
pub enum SyncDirection {
    /// Ascending timestamps (older -> newer).
    /// Used for catching up from a known position.
    Forward,
    /// Descending timestamps (newer -> older).
    /// Used for loading the latest messages first and scrolling backward.
    Backward,
}

/// Bidirectional content pagination request.
///
/// The responder uses its [`TimeIndex`] to serve events around
/// `cursor_ts` in the requested direction, up to `limit` events.
/// This is the primary message for lazy content fetching - the
/// requester already has headers (DAG structure) and wants bodies.
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct RangeReq {
    /// Which DAG to query (genesis timestamp as string).
    pub dag_name: String,
    /// Timestamp cursor. Use `u64::MAX` for "start from newest"
    /// or `0` for "start from oldest".
    pub cursor_ts: u64,
    /// Which direction to paginate.
    pub direction: SyncDirection,
    /// Maximum number of events to return.
    pub limit: u32,
}
impl_p2p_message!(RangeReq, "EventGraph::RangeReq", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Reply to a [`RangeReq`] with events in the requested order.
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct RangeRep(pub Vec<Event>);
impl_p2p_message!(RangeRep, "EventGraph::RangeRep", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// Per-connection protocol handler for the Event Graph.
///
/// One instance is created for each peer connection. It subscribes
/// to all Event Graph P2P message types and spawns async tasks for:
///
/// * `handle_event_put` - real-time ingestion of new events,
///   including RLN proof verification and recursive parent fetching
///   (bounded by [`MAX_PARENT_FETCH_DEPTH`]).
/// * `handle_static_put` - RLN registration and slashing events.
/// * `handle_event_req` - serving event content to peers (only
///   for IDs we've previously broadcast, to prevent DAG enumeration).
/// * `handle_header_req` - serving headers the peer is missing.
/// * `handle_tip_req` - serving our unreferenced tips.
/// * `handle_range_req` - serving bidirectional paginated content
///   (the primary mechanism for lazy content fetching).
/// * `broadcast_rate_limiter` - rate-limiting outbound event
///   relay through a bounded channel with adaptive sleep.
///
/// RLN share metadata is **not** stored on this struct - it lives on
/// [`EventGraph::rln_state`] so that duplicate/reuse detection works
/// across all peer connections, not just the one that relayed a
/// particular event.
pub struct ProtocolEventGraph {
    channel: ChannelPtr,
    event_graph: EventGraphPtr,
    ev_put_sub: MessageSubscription<EventPut>,
    st_put_sub: MessageSubscription<StaticPut>,
    ev_req_sub: MessageSubscription<EventReq>,
    ev_rep_sub: MessageSubscription<EventRep>,
    _hdr_put_sub: MessageSubscription<HeaderPut>,
    hdr_req_sub: MessageSubscription<HeaderReq>,
    _hdr_rep_sub: MessageSubscription<HeaderRep>,
    tip_req_sub: MessageSubscription<TipReq>,
    _tip_rep_sub: MessageSubscription<TipRep>,
    range_req_sub: MessageSubscription<RangeReq>,
    _range_rep_sub: MessageSubscription<RangeRep>,
    malicious_count: AtomicUsize,
    jobsman: ProtocolJobsManagerPtr,
    broadcaster_push: smol::channel::Sender<EventPut>,
    broadcaster_pull: smol::channel::Receiver<EventPut>,
}

#[async_trait]
impl ProtocolBase for ProtocolEventGraph {
    async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        self.jobsman.clone().start(ex.clone());
        self.jobsman.clone().spawn(self.clone().handle_event_put(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_static_put(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_event_req(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_header_req(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_tip_req(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_range_req(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().broadcast_rate_limiter(), ex.clone()).await;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolEventGraph"
    }
}

impl ProtocolEventGraph {
    /// Register message dispatchers and subscribe to all channels.
    pub async fn init(eg: EventGraphPtr, channel: ChannelPtr) -> Result<ProtocolBasePtr> {
        let msg_subsystem = channel.message_subsystem();
        msg_subsystem.add_dispatch::<EventPut>().await;
        msg_subsystem.add_dispatch::<StaticPut>().await;
        msg_subsystem.add_dispatch::<EventReq>().await;
        msg_subsystem.add_dispatch::<EventRep>().await;
        msg_subsystem.add_dispatch::<HeaderPut>().await;
        msg_subsystem.add_dispatch::<HeaderReq>().await;
        msg_subsystem.add_dispatch::<HeaderRep>().await;
        msg_subsystem.add_dispatch::<TipReq>().await;
        msg_subsystem.add_dispatch::<TipRep>().await;
        msg_subsystem.add_dispatch::<RangeReq>().await;
        msg_subsystem.add_dispatch::<RangeRep>().await;

        let (push, pull) = smol::channel::bounded(BROADCASTER_CAPACITY);

        Ok(Arc::new(Self {
            channel: channel.clone(),
            event_graph: eg,
            ev_put_sub: channel.subscribe_msg().await?,
            st_put_sub: channel.subscribe_msg().await?,
            ev_req_sub: channel.subscribe_msg().await?,
            ev_rep_sub: channel.subscribe_msg().await?,
            _hdr_put_sub: channel.subscribe_msg().await?,
            hdr_req_sub: channel.subscribe_msg().await?,
            _hdr_rep_sub: channel.subscribe_msg().await?,
            tip_req_sub: channel.subscribe_msg().await?,
            _tip_rep_sub: channel.subscribe_msg().await?,
            range_req_sub: channel.subscribe_msg().await?,
            _range_rep_sub: channel.subscribe_msg().await?,
            malicious_count: AtomicUsize::new(0),
            jobsman: ProtocolJobsManager::new("ProtocolEventGraph", channel),
            broadcaster_push: push,
            broadcaster_pull: pull,
        }))
    }

    /// Increment the malicious counter; drop peer if threshold reached.
    async fn strike(self: Arc<Self>) -> Result<()> {
        let n = self.malicious_count.fetch_add(1, SeqCst);
        if n + 1 >= MALICIOUS_THRESHOLD {
            error!(
                target: "event_graph::protocol",
                "[EVENTGRAPH] Peer {} reached malicious threshold",
                self.channel.display_address(),
            );
            self.channel.stop().await;
            return Err(Error::ChannelStopped)
        }

        warn!(
            target: "event_graph::protocol",
            "[EVENTGRAPH] Peer {} sent malicious data ({}/{})",
            self.channel.display_address(), n + 1, MALICIOUS_THRESHOLD,
        );

        Ok(())
    }

    async fn handle_event_put(self: Arc<Self>) -> Result<()> {
        let mut bantimes = MovingWindow::new(WINDOW_EXPIRY_TIME);

        loop {
            let (event, blob) = match self.ev_put_sub.receive().await {
                Ok(v) => (v.0.clone(), v.1.clone()),
                Err(_) => continue,
            };

            if !self.event_graph.is_synced() {
                continue
            }

            // RLN: every non-genesis event MUST carry a valid signal
            // proof. The only exception is genesis-shaped events
            // (parents == NULL_PARENTS), which are produced by
            // `dag_prune` on rotation and don't represent user
            // signals. An empty blob on a non-genesis event is an
            // unauthenticated injection attempt - strike the peer
            // and drop the event.
            if event.header.parents != NULL_PARENTS {
                if blob.is_empty() {
                    self.clone().strike().await?;
                    continue
                }
                if self.verify_rln_signal(&event, &blob).await {
                    continue
                }
            } else if !blob.is_empty() {
                // A genesis-shaped event with a non-empty blob is
                // also misbehavior - genesis events are deterministic
                // and don't carry signals. Strike.
                self.clone().strike().await?;
                continue
            }

            _ = self.ev_rep_sub.clean().await;

            // Extract genesis info and immediately release the lock
            let genesis_ts = self.event_graph.current_genesis.read().await.header.timestamp;
            let dag_name = genesis_ts.to_string();
            let eid = event.id();

            // Already known?
            {
                let store = self.event_graph.dag_store.read().await;
                if let Some(slot) = store.get_slot(&genesis_ts) {
                    if slot.header_tree.contains_key(eid.as_bytes()).unwrap_or(false) {
                        continue
                    }
                }
            }

            // Flood protection
            bantimes.ticktock();
            if bantimes.count() > WINDOW_MAXSIZE {
                self.channel.ban().await;
                return Err(Error::MaliciousFlood)
            }

            // Reject events from before the current rotation period
            if event.header.timestamp < genesis_ts {
                continue
            }

            // Quick structural validation
            if !event.validate_new() {
                self.clone().strike().await?;
                continue
            }

            // Fetch missing parents (depth-bounded)
            // See MAX_PARENT_FETCH_DEPTH doc for why this is bounded.
            let mut missing = HashSet::new();
            {
                let store = self.event_graph.dag_store.read().await;
                if let Some(slot) = store.get_slot(&genesis_ts) {
                    for pid in event.header.parents.iter() {
                        if *pid != NULL_ID &&
                            !slot.header_tree.contains_key(pid.as_bytes()).unwrap_or(true)
                        {
                            missing.insert(*pid);
                        }
                    }
                }
            }

            if !missing.is_empty() &&
                !self.clone().fetch_parents(&mut missing, &dag_name, genesis_ts).await
            {
                // Depth exceeded or peer misbehaved
                continue
            }

            // Insert the event itself. We use plain dag_insert
            // (not dag_insert_with_blobs) because the blob has
            // already been verified above (the verify_rln_signal
            // gate). Re-verifying via dag_insert_with_blobs would
            // produce a spurious "duplicate share" rejection,
            // because rln_verify_signal recorded the share on the
            // first call.
            //
            // We store the blob in the side-table separately, so
            // future late-joiners can re-verify it during sync.
            // This mirrors the originator path in nickserv.rs that
            // calls static_blob_store after static_insert.
            if self
                .event_graph
                .header_dag_insert(vec![event.header.clone()], &dag_name)
                .await
                .is_err()
            {
                self.clone().strike().await?;
                continue
            }

            if self.event_graph.dag_insert(slice::from_ref(&event), &dag_name).await.is_err() {
                self.clone().strike().await?;
                continue
            }

            // Persist the verified blob alongside the event for
            // sync-time re-verification by future late-joiners.
            let _ = self.event_graph.dag_blob_store(&event.id(), &blob);

            // Relay to other peers (bounded - drops if channel full)
            let _ = self.broadcaster_push.try_send(EventPut(event, blob));
        }
    }

    /// Recursively fetch missing parent events, up to
    /// [`MAX_PARENT_FETCH_DEPTH`] rounds.
    /// Returns `true` on success.
    async fn fetch_parents(
        self: Arc<Self>,
        missing: &mut HashSet<blake3::Hash>,
        dag_name: &str,
        dag_ts: u64,
    ) -> bool {
        // received[layer] = Vec<(event, blob)> - keeping events
        // paired with their blobs through the layer ordering so we
        // can re-verify proofs at insert time. An empty blob means
        // the serving peer didn't have one; dag_insert_with_blobs
        // treats that as the trust-the-quorum fallback.
        let mut received: BTreeMap<u64, Vec<(Event, Vec<u8>)>> = BTreeMap::new();
        let mut known = HashSet::new();
        let mut depth = 0usize;

        while !missing.is_empty() {
            depth += 1;
            if depth > MAX_PARENT_FETCH_DEPTH {
                error!(
                    target: "event_graph::protocol",
                    "[EVENTGRAPH] Parent fetch depth exceeded ({})",
                    MAX_PARENT_FETCH_DEPTH,
                );
                let _ = self.clone().strike().await;
                return false
            }

            if self.channel.send(&EventReq(missing.iter().cloned().collect())).await.is_err() {
                return false
            }

            let timeout =
                self.event_graph.p2p.settings().read().await.outbound_connect_timeout_max();

            let Ok(rep) = self.ev_rep_sub.receive_with_timeout(timeout).await else {
                self.channel.stop().await;
                return false
            };

            // Pair each returned event with its corresponding blob.
            let parents = rep.0.clone();
            let blobs_in = rep.1.clone();
            let blobs_aligned = blobs_in.len() == parents.len();
            for (i, parent) in parents.into_iter().enumerate() {
                let pid = parent.id();
                if !missing.contains(&pid) {
                    // Peer sent an event we didn't ask for
                    self.channel.stop().await;
                    return false
                }
                let blob = if blobs_aligned { blobs_in[i].clone() } else { Vec::new() };
                received.entry(parent.header.layer).or_default().push((parent.clone(), blob));
                known.insert(pid);
                missing.remove(&pid);

                // Check for more unknown grandparents
                let store = self.event_graph.dag_store.read().await;
                if let Some(slot) = store.get_slot(&dag_ts) {
                    for gp in parent.header.parents.iter() {
                        if *gp != NULL_ID &&
                            !missing.contains(gp) &&
                            !known.contains(gp) &&
                            !slot.header_tree.contains_key(gp.as_bytes()).unwrap_or(true)
                        {
                            missing.insert(*gp);
                        }
                    }
                }
            }
        }

        // Flatten in layer order so parents are inserted before
        // children (dag_insert structurally validates parents).
        let pairs: Vec<(Event, Vec<u8>)> = received.into_values().flatten().collect();
        let events: Vec<Event> = pairs.iter().map(|(e, _)| e.clone()).collect();
        let blobs: Vec<Vec<u8>> = pairs.iter().map(|(_, b)| b.clone()).collect();
        let headers: Vec<Header> = events.iter().map(|e| e.header.clone()).collect();

        if self.event_graph.header_dag_insert(headers, dag_name).await.is_err() {
            return false
        }

        // dag_insert_with_blobs verifies each event's blob (when
        // present) and skips events that fail RLN re-verification.
        // This closes sync-time injection via fetch_parents.
        if self.event_graph.dag_insert_with_blobs(&events, &blobs, dag_name).await.is_err() {
            return false
        }

        true
    }

    /// Verify an RLN signal proof. Returns `true` if the event
    /// should be rejected (proof invalid, duplicate, or slashable).
    ///
    /// The actual verification logic lives on
    /// [`EventGraph::rln_verify_signal`] - this method is a thin
    /// wrapper that translates the [`rln::SignalCheck`] outcome
    /// into "accept or reject" plus the slash side effect.
    async fn verify_rln_signal(&self, event: &Event, blob: &[u8]) -> bool {
        match self.event_graph.rln_verify_signal(event, blob).await {
            rln::SignalCheck::Accepted => false,
            rln::SignalCheck::Rejected => true,
            rln::SignalCheck::Slashable(shares) => {
                self.slash(shares).await;
                true
            }
        }
    }

    /// Execute the slashing procedure: SSS-recover `identity_secret_hash`
    /// from the conflicting shares, derive the corresponding commitment
    /// directly, and broadcast the slash proof.
    ///
    /// With the spec-aligned signal circuit (`a_0 = identity_secret_hash`),
    /// the recovered value uniquely determines the commitment via a
    /// single Poseidon hash. The previous brute-force loop over
    /// `1..=MAX_MSG_LIMIT` is gone - `user_message_limit` is already
    /// baked into `identity_secret_hash`, so the verifier doesn't
    /// need to know it explicitly.
    async fn slash(&self, shares: Vec<(pallas::Base, pallas::Base)>) {
        let identity_secret_hash = match sss_recover(&shares) {
            Ok(s) => s,
            Err(e) => {
                error!(target: "event_graph::protocol", "[RLN] SSS recovery failed: {e}");
                return
            }
        };

        let commitment = poseidon_hash([identity_secret_hash]);

        // Sanity check: the commitment we recovered must be in the
        // tree. If it isn't, something has gone wrong (the proofs
        // were against a stale root we no longer have, or we have a
        // bug). Either way, do not broadcast a bogus slash.
        {
            let id_state = self.event_graph.identity_state.read().await;
            if !id_state.contains(&commitment) {
                warn!(
                    target: "event_graph::protocol",
                    "[RLN] Recovered commitment is not a current tree leaf; skipping slash",
                );
                return
            }
        }

        let slash_pk = match self.event_graph.zk_keys.load_slash_pk() {
            Ok(pk) => pk,
            Err(e) => {
                error!(target: "event_graph::protocol", "[RLN] Failed to load slash PK: {e}");
                return
            }
        };

        let mut id = self.event_graph.identity_state.write().await;
        let (proof, root) = match create_slash_proof(identity_secret_hash, &mut id, &slash_pk) {
            Ok(v) => v,
            Err(e) => {
                error!(target: "event_graph::protocol", "[RLN] Slash proof creation failed: {e}");
                return
            }
        };
        // Note: create_slash_proof itself does NOT mutate the SMT -
        // it only reads the membership path. The actual removal happens
        // when `static_insert` propagates the slashing event through
        // `handle_static_put`, the same code path remote slashes use.
        drop(id);

        let slash_blob = SlashBlob { proof, identity_secret_hash, merkle_root: root };
        let blob = serialize_async(&slash_blob).await;
        let node = RLNNode::Slashing(commitment);
        let ev = Event::new_static(serialize_async(&node).await, &self.event_graph).await;
        if let Err(e) = self.event_graph.commit_verified_static_event(&ev, &blob, &node).await {
            error!(target: "event_graph::protocol", "[RLN] Slash static commit failed: {e}");
            return
        }
        let _ = self.event_graph.static_broadcast(ev, blob).await;
    }

    async fn handle_static_put(self: Arc<Self>) -> Result<()> {
        let mut bantimes = MovingWindow::new(WINDOW_EXPIRY_TIME);
        loop {
            let (event, blob) = match self.st_put_sub.receive().await {
                Ok(v) => (v.0.clone(), v.1.clone()),
                Err(_) => continue,
            };
            if !self.event_graph.is_synced() {
                continue
            }
            let eid = event.id();
            if self.event_graph.static_dag.contains_key(eid.as_bytes())? {
                continue
            }

            // Validate event structure and parents BEFORE touching
            // the identity tree.
            bantimes.ticktock();
            if bantimes.count() > WINDOW_MAXSIZE {
                self.channel.ban().await;
                return Err(Error::MaliciousFlood)
            }
            if !event.validate_new() {
                self.clone().strike().await?;
                continue
            }
            let mut orphan = false;
            for p in event.header.parents.iter() {
                if *p != NULL_ID && !self.event_graph.static_dag.contains_key(p.as_bytes())? {
                    orphan = true;
                    break
                }
            }
            if orphan {
                self.clone().strike().await?;
                continue
            }

            let rln_node: RLNNode = match deserialize_async_partial(event.content()).await {
                Ok((v, _)) => v,
                Err(_) => continue,
            };
            if blob.is_empty() {
                continue
            }

            // Decision is made by EventGraph::rln_verify_static_event,
            // a pure verification function (no state mutation). Accepted events
            // are committed through one helper so the static DAG is durable
            // before RLN state, while subscribers are notified only after the
            // RLN apply step.
            match self
                .event_graph
                .rln_verify_static_event(&rln_node, &blob, event.header.timestamp)
                .await
            {
                rln::StaticEventCheck::AcceptedRegistration(_) |
                rln::StaticEventCheck::AcceptedSlash(_) => {
                    if let Err(e) = self
                        .event_graph
                        .commit_verified_static_event(&event, &blob, &rln_node)
                        .await
                    {
                        warn!(
                            target: "event_graph::protocol",
                            "[RLN] commit_verified_static_event failed: {e}",
                        );
                        continue
                    }
                }
                rln::StaticEventCheck::Rejected => continue,
                rln::StaticEventCheck::Malicious => {
                    self.clone().strike().await?;
                    continue
                }
            }

            self.event_graph.static_broadcast(event, blob).await?;
        }
    }

    async fn handle_event_req(self: Arc<Self>) -> Result<()> {
        loop {
            let ids = match self.ev_req_sub.receive().await {
                Ok(v) => v.0.clone(),
                Err(_) => continue,
            };
            if !self.event_graph.is_synced() {
                continue
            }
            if ids.len() > MAX_EVENT_REQ_IDS {
                self.clone().strike().await?;
                continue
            }

            // Only serve IDs we've previously broadcast (prevents
            // arbitrary DAG enumeration by malicious peers). The
            // static DAG is exempt from this check because its
            // contents are public consensus state (RLN registrations
            // and slashes) - serving those freely has no privacy
            // cost, and it's required so that a peer performing
            // `static_sync` can walk ancestry via EventReq.
            //
            // For both static and rotating-DAG events, we include
            // the original RLN blob (proof + public inputs + ...)
            // so the requester can re-verify the proof at sync time.
            // The blobs vector is index-aligned with `events`; an
            // empty entry means we don't have the blob, which can
            // legitimately happen for events inserted before the
            // current blob-storage code paths existed.
            let bcast = self.event_graph.broadcasted_ids.read().await;
            let mut events = vec![];
            let mut blobs: Vec<Vec<u8>> = vec![];
            for id in &ids {
                let in_static =
                    self.event_graph.static_dag.contains_key(id.as_bytes()).unwrap_or(false);
                if !in_static && !bcast.contains(id) {
                    self.clone().strike().await?;
                    continue
                }
                if let Some(ev) = self.event_graph.fetch_event_from_dags(id).await? {
                    let blob = if in_static {
                        self.event_graph.static_blob_fetch(id).unwrap_or(None).unwrap_or_default()
                    } else {
                        self.event_graph.dag_blob_fetch(id).unwrap_or(None).unwrap_or_default()
                    };
                    if !in_static && blob.is_empty() {
                        // Rotating-DAG event without a blob. Don't ship
                        // it, just log loudly.
                        warn!(
                            target: "event_graph::handle_event_req",
                            "[EVENTGRAPH] declining to serve event {} - missing blob in \
                            local dag_blobs",
                            id,
                        );
                        continue
                    }
                    events.push(ev);
                    blobs.push(blob);
                }
            }
            drop(bcast);

            if !events.is_empty() {
                let mut b = self.event_graph.broadcasted_ids.write().await;
                for ev in &events {
                    for p in ev.header.parents.iter() {
                        if *p != NULL_ID {
                            b.insert(*p);
                        }
                    }
                }
                drop(b);
                self.channel.send(&EventRep(events, blobs)).await?;
            }
        }
    }

    async fn handle_header_req(self: Arc<Self>) -> Result<()> {
        loop {
            let Ok(v) = self.hdr_req_sub.receive().await else { continue };
            if !self.event_graph.is_synced() {
                continue
            }
            let (dag_name, tips) = (&v.0, &v.1);
            if count_layer_tips(tips) > MAX_HEADER_REQ_TIPS {
                self.clone().strike().await?;
                continue
            }
            let dag_ts = match u64::from_str(dag_name) {
                Ok(v) => v,
                Err(_) => continue,
            };
            {
                let s = self.event_graph.dag_store.read().await;
                if s.get_slot(&dag_ts).is_none() {
                    continue
                }
            }
            let hdrs = self.event_graph.fetch_headers_with_tips(dag_name, tips).await?;

            // Register every header ID we are about to reveal as
            // "broadcasted." A peer that just received these headers
            // via HeaderRep will follow up with EventReq for the
            // bodies. `handle_event_req` strikes any peer that asks
            // for an ID not present in `broadcasted_ids` (the check
            // exists to prevent arbitrary DAG enumeration). Without
            // this insert the legitimate header-then-body sync flow
            // racks up MALICIOUS_THRESHOLD strikes per batch and the
            // peer gets dropped.
            //
            // `handle_tip_req` already does the analogous insert for
            // the tip IDs it ships, and `handle_event_req` does it
            // for the parents of events it serves. This fills the
            // missing third side of the same pattern.
            {
                let mut b = self.event_graph.broadcasted_ids.write().await;
                for h in &hdrs {
                    b.insert(h.id());
                }
            }

            self.channel.send(&HeaderRep(hdrs)).await?;
        }
    }

    async fn handle_tip_req(self: Arc<Self>) -> Result<()> {
        loop {
            let dag_name = match self.tip_req_sub.receive().await {
                Ok(v) => v.0.clone(),
                Err(_) => continue,
            };
            if !self.event_graph.is_synced() {
                continue
            }

            let layers = match dag_name.as_str() {
                "static-dag" => self.event_graph.static_unreferenced_tips().await,
                _ => {
                    let ts = match u64::from_str(&dag_name) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let store = self.event_graph.dag_store.read().await;
                    match store.get_slot(&ts) {
                        Some(s) => s.tips.clone(),
                        None => continue,
                    }
                }
            };

            let layers = cap_layer_tips(&layers, MAX_TIP_REP_TIPS);

            let mut b = self.event_graph.broadcasted_ids.write().await;
            for tips in layers.values() {
                for t in tips {
                    b.insert(*t);
                }
            }
            drop(b);
            self.channel.send(&TipRep(layers)).await?;
        }
    }

    /// Serve a paginated content request. Uses the local
    /// [`TimeIndex`] to find events around the cursor, then
    /// returns their full content.
    async fn handle_range_req(self: Arc<Self>) -> Result<()> {
        loop {
            let req = match self.range_req_sub.receive().await {
                Ok(v) => v,
                Err(_) => continue,
            };
            if !self.event_graph.is_synced() {
                continue
            }
            let limit = (req.limit as usize).min(MAX_RANGE_PAGE_SIZE);
            let events =
                self.event_graph.fetch_page(req.cursor_ts, req.direction.clone(), limit).await?;
            self.channel.send(&RangeRep(events)).await?;
        }
    }

    async fn broadcast_rate_limiter(self: Arc<Self>) -> Result<()> {
        let mut rl = MovingWindow::new(RATELIMIT_EXPIRY_TIME);
        loop {
            let ep = self.broadcaster_pull.recv().await.expect("broadcaster closed");
            rl.ticktock();
            if rl.count() > RATELIMIT_MIN_COUNT {
                let ms = ((rl.count() - RATELIMIT_MIN_COUNT) * RATELIMIT_SAMPLE_SLEEP /
                    (RATELIMIT_SAMPLE_IDX - RATELIMIT_MIN_COUNT)) as u64;
                msleep(ms).await;
            }
            self.event_graph
                .p2p
                .broadcast_with_exclude(&ep, &[self.channel.address().clone()])
                .await;
        }
    }
}
