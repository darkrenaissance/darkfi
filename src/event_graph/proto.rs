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
    rln::{closest_epoch, create_slash_proof, hash_event, sss_recover, Blob, RLNNode, RlnState},
    Event, EventGraphPtr, LayerUTips, NULL_ID,
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
    zk::Proof,
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
/// Reference point for computing sleep time: when count = this value…
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
                } // future timestamp — remove
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

/// Reply with full events.
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct EventRep(pub Vec<Event>);
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

            // RLN: verify proof BEFORE recording shares
            if !blob.is_empty() && self.verify_rln_signal(&event, &blob).await {
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

            // Insert the event itself
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
        let mut received: BTreeMap<u64, Vec<Event>> = BTreeMap::new();
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

            for parent in rep.0.clone() {
                let pid = parent.id();
                if !missing.contains(&pid) {
                    // Peer sent an event we didn't ask for
                    self.channel.stop().await;
                    return false
                }
                received.entry(parent.header.layer).or_default().push(parent.clone());
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

        // Insert in layer order. We insert into both header_tree and
        // main_tree - inserting into header_tree alone would create
        // an inconsistent state where an event E exists in main_tree
        // but its parent P does not, even though both have headers.
        // Any future ancestor walk via main_tree.get() would hit a
        // None and fail. If the node wants to discard bodies for
        // space, that should be a separate pruning pass, not a
        // sync-time partial-insert.
        let events: Vec<Event> = received.into_values().flatten().collect();
        let headers: Vec<Header> = events.iter().map(|e| e.header.clone()).collect();

        if self.event_graph.header_dag_insert(headers, dag_name).await.is_err() {
            return false
        }

        if self.event_graph.dag_insert(&events, dag_name).await.is_err() {
            return false
        }

        true
    }

    /// Verify an RLN signal proof. Returns `true` if the event
    /// should be rejected (proof invalid, duplicate, or slashable).
    async fn verify_rln_signal(&self, event: &Event, blob: &[u8]) -> bool {
        let rcvd: Blob = match deserialize_async_partial(blob).await {
            Ok((v, _)) => v,
            Err(_) => return true, // unparseable blob -> reject
        };

        let epoch = closest_epoch(event.header.timestamp);
        let ext_null = poseidon_hash([pallas::Base::from(epoch), pallas::Base::from(1000)]);
        let x = hash_event(event);
        let root = self.event_graph.identity_state.read().await.root();
        let pi = vec![root, ext_null, x, rcvd.y, rcvd.internal_nullifier];

        // Global metadata check
        {
            let mut rln = self.event_graph.rln_state.write().await;
            if rln.current_epoch != epoch {
                *rln = RlnState::new();
                rln.current_epoch = epoch;
            }

            if rln.metadata.is_duplicate(&ext_null, &rcvd.internal_nullifier, &x, &rcvd.y) {
                return true
            }

            if rln.metadata.is_reused(&ext_null, &rcvd.internal_nullifier) {
                let shares = rln.metadata.get_shares(&ext_null, &rcvd.internal_nullifier);
                drop(rln);
                self.slash(shares, rcvd.user_msg_limit).await;
                return true
            }
        }

        // Verify proof using cached VK
        if rcvd.proof.verify(&self.event_graph.zk_keys.signal_vk, &pi).is_err() {
            return true
        }

        // Proof valid -> record share
        let mut rln = self.event_graph.rln_state.write().await;
        let _ = rln.metadata.add_share(ext_null, rcvd.internal_nullifier, x, rcvd.y);
        false
    }

    /// Execute the slashing procedure: recover the secret, load the
    /// slash proving key from sled, produce a slash proof, and
    /// broadcast the slashing event.
    async fn slash(&self, shares: Vec<(pallas::Base, pallas::Base)>, limit: u64) {
        let secret = match sss_recover(&shares) {
            Ok(s) => s,
            Err(e) => {
                error!(
                    target: "event_graph::slash",
                    "[RLN] SSS recovery failed: {e}",
                );
                return
            }
        };

        // Lazy-load the slash PK from sled
        let slash_pk = match self.event_graph.zk_keys.load_slash_pk() {
            Ok(pk) => pk,
            Err(e) => {
                error!(
                    target: "event_graph::slash",
                    "[RLN] Failed to load slash PK: {e}",
                );
                return
            }
        };

        let mut id = self.event_graph.identity_state.write().await;
        let (proof, root) = match create_slash_proof(secret, limit, &mut id, &slash_pk) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "event_graph::slash",
                    "[RLN] Slash proof creation failed: {e}",
                );
                return
            }
        };
        drop(id);

        let blob = serialize_async(&(proof, secret, limit, root)).await;
        let commitment = poseidon_hash([poseidon_hash([secret, limit.into()])]);
        let node = RLNNode::Slashing(commitment);
        let ev = Event::new_static(serialize_async(&node).await, &self.event_graph).await;
        let _ = self.event_graph.static_insert(&ev).await;
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

            let rln_node: RLNNode = match deserialize_async_partial(event.content()).await {
                Ok((v, _)) => v,
                Err(_) => continue,
            };
            if blob.is_empty() {
                continue
            }

            match rln_node {
                RLNNode::Registration(commitment) => {
                    let (proof, msg_limit): (Proof, u64) =
                        match deserialize_async_partial(&blob).await {
                            Ok((v, _)) => v,
                            Err(_) => continue,
                        };
                    if proof
                        .verify(
                            &self.event_graph.zk_keys.register_vk,
                            &[commitment, msg_limit.into()],
                        )
                        .is_err()
                    {
                        continue
                    }
                    // Persist the new identity
                    if let Err(e) =
                        self.event_graph.identity_state.write().await.register(commitment)
                    {
                        error!("[RLN] Register: {e}");
                        continue
                    }
                }
                RLNNode::Slashing(commitment) => {
                    let (proof, secret, msg_limit, root): (Proof, pallas::Base, u64, pallas::Base) =
                        match deserialize_async_partial(&blob).await {
                            Ok((v, _)) => v,
                            Err(_) => continue,
                        };
                    if proof
                        .verify(
                            &self.event_graph.zk_keys.slash_vk,
                            &[secret, msg_limit.into(), root],
                        )
                        .is_err()
                    {
                        continue
                    }
                    let rebuilt = poseidon_hash([poseidon_hash([secret, msg_limit.into()])]);
                    if commitment != rebuilt {
                        self.clone().strike().await?;
                        continue
                    }
                    if let Err(e) = self.event_graph.identity_state.write().await.slash(rebuilt) {
                        error!("[RLN] Slash: {e}");
                        continue
                    }
                }
            }

            // Validate parents exist in static DAG
            for p in event.header.parents.iter() {
                if *p != NULL_ID && !self.event_graph.static_dag.contains_key(p.as_bytes())? {
                    return Err(Error::EventNotFound("Orphan static event".into()))
                }
            }

            bantimes.ticktock();
            if bantimes.count() > WINDOW_MAXSIZE {
                self.channel.ban().await;
                return Err(Error::MaliciousFlood)
            }
            if !event.validate_new() {
                self.clone().strike().await?;
                continue
            }

            self.event_graph.static_insert(&event).await?;
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

            // Only serve IDs we've previously broadcast (prevents
            // arbitrary DAG enumeration by malicious peers).
            let bcast = self.event_graph.broadcasted_ids.read().await;
            let mut events = vec![];
            for id in &ids {
                if !bcast.contains(id) {
                    self.clone().strike().await?;
                    continue
                }
                if let Some(ev) = self.event_graph.fetch_event_from_dags(id).await? {
                    events.push(ev);
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
                self.channel.send(&EventRep(events)).await?;
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
            let events = self
                .event_graph
                .fetch_page(req.cursor_ts, req.direction.clone(), req.limit as usize)
                .await?;
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
