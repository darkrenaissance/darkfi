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

use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    slice,
    str::FromStr,
    sync::{
        atomic::{AtomicUsize, Ordering::SeqCst},
        Arc,
    },
};

use darkfi_sdk::{
    crypto::{poseidon_hash, util::FieldElemAsStr},
    pasta::{pallas, Fp},
};
use darkfi_serial::{
    async_trait, deserialize_async, deserialize_async_partial, serialize_async, SerialDecodable,
    SerialEncodable,
};
use smol::Executor;
use tracing::{debug, error, info, trace, warn};

use super::{event::Header, Event, EventGraphPtr, LayerUTips, NULL_ID, NULL_PARENTS};
use crate::{
    event_graph::util::{
        closest_epoch, create_slash_proof, hash_event, sss_recover, MessageMetadata,
    },
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

/// Malicious behaviour threshold. If the threshold is reached, we will
/// drop the peer from our P2P connection.
const MALICIOUS_THRESHOLD: usize = 5;

/// Global limit of messages per window
const WINDOW_MAXSIZE: usize = 200;
/// Rolling length of the window
const WINDOW_EXPIRY_TIME: NanoTimestamp = NanoTimestamp::from_secs(60);

/// Rolling length of the window
const RATELIMIT_EXPIRY_TIME: NanoTimestamp = NanoTimestamp::from_secs(10);
/// Ratelimit kicks in above this count
const RATELIMIT_MIN_COUNT: usize = 6;
/// Sample point used to calculate sleep time when ratelimit is active
const RATELIMIT_SAMPLE_IDX: usize = 10;
/// Sleep for this amount of time when `count == RATE_LIMIT_SAMPLE_IDX`.
const RATELIMIT_SAMPLE_SLEEP: usize = 1000;

struct MovingWindow {
    times: VecDeque<NanoTimestamp>,
    expiry_time: NanoTimestamp,
}

impl MovingWindow {
    fn new(expiry_time: NanoTimestamp) -> Self {
        Self { times: VecDeque::new(), expiry_time }
    }

    /// Clean out expired timestamps from the window.
    fn clean(&mut self) {
        while let Some(ts) = self.times.front() {
            let Ok(elapsed) = ts.elapsed() else {
                debug!(target: "event_graph::protocol::MovingWindow::clean", "Timestamp [{ts}] is in future. Removing...");
                let _ = self.times.pop_front();
                continue
            };
            if elapsed < self.expiry_time {
                break
            }
            let _ = self.times.pop_front();
        }
    }

    /// Add new timestamp
    fn ticktock(&mut self) {
        self.clean();
        self.times.push_back(NanoTimestamp::current_time());
    }

    #[inline]
    fn count(&self) -> usize {
        self.times.len()
    }
}

/// P2P protocol implementation for the Event Graph.
pub struct ProtocolEventGraph {
    /// Pointer to the connected peer
    channel: ChannelPtr,
    /// Pointer to the Event Graph instance
    event_graph: EventGraphPtr,
    /// `MessageSubscriber` for `EventPut`
    ev_put_sub: MessageSubscription<EventPut>,
    /// `MessageSubscriber` for `StaticPut`
    st_put_sub: MessageSubscription<StaticPut>,
    /// `MessageSubscriber` for `EventReq`
    ev_req_sub: MessageSubscription<EventReq>,
    /// `MessageSubscriber` for `EventRep`
    ev_rep_sub: MessageSubscription<EventRep>,
    /// `MessageSubscriber` for `HeaderPut`
    _hdr_put_sub: MessageSubscription<HeaderPut>,
    /// `MessageSubscriber` for `HeaderReq`
    hdr_req_sub: MessageSubscription<HeaderReq>,
    /// `MessageSubscriber` for `HeaderRep`
    _hdr_rep_sub: MessageSubscription<HeaderRep>,
    /// `MessageSubscriber` for `TipReq`
    tip_req_sub: MessageSubscription<TipReq>,
    /// `MessageSubscriber` for `TipRep`
    _tip_rep_sub: MessageSubscription<TipRep>,
    /// Peer malicious message count
    malicious_count: AtomicUsize,
    /// P2P jobs manager pointer
    jobsman: ProtocolJobsManagerPtr,
    /// To apply the rate-limit, we don't broadcast directly but instead send into the
    /// sending queue.
    broadcaster_push: smol::channel::Sender<EventPut>,
    /// Receive send requests and rate-limit broadcasting them.
    broadcaster_pull: smol::channel::Receiver<EventPut>,
}

/// A P2P message representing publishing an event on the network
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct EventPut(pub Event, pub Vec<u8>);
impl_p2p_message!(EventPut, "EventGraph::EventPut", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// A P2P message representing publishing an event of a static graph
/// (most likely RLN_identities) on the network
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct StaticPut(pub Event, pub Vec<u8>);
impl_p2p_message!(StaticPut, "EventGraph::StaticPut", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// A P2P message representing an event request
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct EventReq(pub Vec<blake3::Hash>);
impl_p2p_message!(EventReq, "EventGraph::EventReq", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// A P2P message representing an event reply
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct EventRep(pub Vec<Event>);
impl_p2p_message!(EventRep, "EventGraph::EventRep", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// A P2P message representing publishing an event's header on the network
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct HeaderPut(pub Header);
impl_p2p_message!(HeaderPut, "EventGraph::HeaderPut", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// A P2P message representing a header request
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct HeaderReq(pub String);
impl_p2p_message!(HeaderReq, "EventGraph::HeaderReq", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// A P2P message representing a header reply
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct HeaderRep(pub Vec<Header>);
impl_p2p_message!(HeaderRep, "EventGraph::HeaderRep", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// A P2P message representing a request for a peer's DAG tips
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct TipReq(pub String);
impl_p2p_message!(TipReq, "EventGraph::TipReq", 0, 0, DEFAULT_METERING_CONFIGURATION);

/// A P2P message representing a reply for the peer's DAG tips
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct TipRep(pub LayerUTips);
impl_p2p_message!(TipRep, "EventGraph::TipRep", 0, 0, DEFAULT_METERING_CONFIGURATION);

#[async_trait]
impl ProtocolBase for ProtocolEventGraph {
    async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        self.jobsman.clone().start(ex.clone());
        self.jobsman.clone().spawn(self.clone().handle_event_put(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_static_put(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_event_req(), ex.clone()).await;
        // self.jobsman.clone().spawn(self.clone().handle_header_put(), ex.clone()).await;
        // self.jobsman.clone().spawn(self.clone().handle_header_req(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_header_req(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_tip_req(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().broadcast_rate_limiter(), ex.clone()).await;
        Ok(())
    }

    fn name(&self) -> &'static str {
        "ProtocolEventGraph"
    }
}

impl ProtocolEventGraph {
    pub async fn init(event_graph: EventGraphPtr, channel: ChannelPtr) -> Result<ProtocolBasePtr> {
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

        let ev_put_sub = channel.subscribe_msg::<EventPut>().await?;
        let st_put_sub = channel.subscribe_msg::<StaticPut>().await?;
        let ev_req_sub = channel.subscribe_msg::<EventReq>().await?;
        let ev_rep_sub = channel.subscribe_msg::<EventRep>().await?;
        let _hdr_put_sub = channel.subscribe_msg::<HeaderPut>().await?;
        let hdr_req_sub = channel.subscribe_msg::<HeaderReq>().await?;
        let _hdr_rep_sub = channel.subscribe_msg::<HeaderRep>().await?;
        let tip_req_sub = channel.subscribe_msg::<TipReq>().await?;
        let _tip_rep_sub = channel.subscribe_msg::<TipRep>().await?;

        let (broadcaster_push, broadcaster_pull) = smol::channel::unbounded();

        Ok(Arc::new(Self {
            channel: channel.clone(),
            event_graph,
            ev_put_sub,
            st_put_sub,
            ev_req_sub,
            ev_rep_sub,
            _hdr_put_sub,
            hdr_req_sub,
            _hdr_rep_sub,
            tip_req_sub,
            _tip_rep_sub,
            malicious_count: AtomicUsize::new(0),
            jobsman: ProtocolJobsManager::new("ProtocolEventGraph", channel.clone()),
            broadcaster_push,
            broadcaster_pull,
        }))
    }

    async fn increase_malicious_count(self: Arc<Self>) -> Result<()> {
        let malicious_count = self.malicious_count.fetch_add(1, SeqCst);
        if malicious_count + 1 == MALICIOUS_THRESHOLD {
            error!(
                target: "event_graph::protocol::handle_event_put",
                "[EVENTGRAPH] Peer {} reached malicious threshold. Dropping connection.",
                self.channel.display_address(),
            );
            self.channel.stop().await;
            return Err(Error::ChannelStopped)
        }

        warn!(
            target: "event_graph::protocol::handle_event_put",
            "[EVENTGRAPH] Peer {} sent us a malicious event", self.channel.display_address(),
        );

        Ok(())
    }

    /// Protocol function handling `EventPut`.
    /// This is triggered whenever someone broadcasts (or relays) a new
    /// event on the network.
    async fn handle_event_put(self: Arc<Self>) -> Result<()> {
        // Rolling window of event timestamps on this channel
        let mut bantimes = MovingWindow::new(WINDOW_EXPIRY_TIME);
        let mut metadata = MessageMetadata::new();
        let mut current_epoch = 0;

        loop {
            let (event, blob) = match self.ev_put_sub.receive().await {
                Ok(v) => (v.0.clone(), v.1.clone()),
                Err(_) => continue,
            };
            trace!(
                 target: "event_graph::protocol::handle_event_put",
                 "Got EventPut: {} [{}]", event.id(), self.channel.display_address(),
            );

            // Check if node has finished syncing its DAG
            if !*self.event_graph.synced.read().await {
                debug!(
                    target: "event_graph::protocol::handle_event_put",
                    "DAG is still syncing, skipping..."
                );
                continue
            }

            let mut verification_failed = false;
            #[allow(clippy::never_loop)]
            loop {
                if blob.is_empty() {
                    break
                }
                let (proof, y, internal_nullifier, user_msg_limit): (
                    Proof,
                    pallas::Base,
                    pallas::Base,
                    u64,
                ) = match deserialize_async_partial(&blob).await {
                    Ok((v, _)) => v,
                    Err(e) => {
                        error!(target: "event_graph::protocol::handle_event_put()","[EVENTGRAPH] Failed deserializing event ephemeral data: {}", e);
                        break
                    }
                };

                // If the current epoch is different, we reset the stored shares
                if current_epoch != closest_epoch(event.header.timestamp) {
                    metadata = MessageMetadata::new()
                }

                let rln_app_identifier = pallas::Base::from(1000);
                current_epoch = closest_epoch(event.header.timestamp);
                let epoch = pallas::Base::from(current_epoch);
                let external_nullifier = poseidon_hash([epoch, rln_app_identifier]);
                let x = hash_event(&event);
                let identity_root = self.event_graph.rln_identity_tree.read().await.root();
                let public_inputs =
                    vec![identity_root, external_nullifier, x, y, internal_nullifier];

                if metadata.is_duplicate(&external_nullifier, &internal_nullifier, &x, &y) {
                    error!(target: "event_graph::protocol::handle_event_put()", "[RLN] Duplicate Message!");
                    verification_failed = true;
                    break
                }

                if metadata.is_reused(&external_nullifier, &internal_nullifier) {
                    info!(target: "event_graph::protocol::handle_event_put()", "[RLN] Metadata is reused.. slashing..");
                    let shares = metadata.get_shares(&external_nullifier, &internal_nullifier);
                    let secret = sss_recover(&shares);

                    // Broadcast slashing event
                    let slash_pk = &self.event_graph.slash_pk;
                    let mut identity_tree = self.event_graph.rln_identity_tree.write().await;

                    info!("[RLN] Creating slashing proof");
                    let (proof, identity_root) = match create_slash_proof(
                        secret,
                        user_msg_limit,
                        &mut identity_tree,
                        slash_pk,
                    ) {
                        Ok(v) => v,
                        Err(e) => {
                            error!("[RLN] Failed creating RLN slash proof: {}", e);
                            // Just use an empty "proof"
                            (Proof::new(vec![]), pallas::Base::from(0))
                        }
                    };
                    drop(identity_tree);

                    let blob =
                        serialize_async(&(proof, secret, user_msg_limit, identity_root)).await;

                    let evgr = &self.event_graph;
                    let st_event = Event::new_static(vec![0, 1], evgr).await;
                    evgr.static_broadcast(st_event, blob).await?;

                    verification_failed = true;
                    break
                }

                // At this point we can safely add the shares
                metadata.add_share(external_nullifier, internal_nullifier, x, y)?;

                info!(target: "event_graph::protocol::handle_event_put()", "[RLN] Verifying incoming Event RLN proof");
                verification_failed =
                    proof.verify(&self.event_graph.signal_vk, &public_inputs).is_err();

                break
            }

            if verification_failed {
                error!(target: "event_graph::protocol::handle_event_put()", "[RLN] Incoming Event RLN Signaling proof verification failed");
                continue
            }

            // If we have already seen the event, we'll stay quiet.
            let current_genesis = self.event_graph.current_genesis.read().await;
            let genesis_timestamp = current_genesis.header.timestamp;
            let dag_name = genesis_timestamp.to_string();
            let hdr_tree_name = format!("headers_{dag_name}");
            let event_id = event.id();
            if self
                .event_graph
                .dag_store
                .read()
                .await
                .get_dag(&hdr_tree_name)
                .contains_key(event_id.as_bytes())
                .unwrap()
            {
                debug!(
                    target: "event_graph::protocol::handle_event_put",
                    "Event {event_id} is already known"
                );
                continue
            }

            // There's a new unique event.
            // Apply ban logic to stop network floods.
            bantimes.ticktock();
            if bantimes.count() > WINDOW_MAXSIZE {
                self.channel.ban().await;
                // This error is actually unused. We could return Ok here too.
                return Err(Error::MaliciousFlood)
            }

            // We received an event. Check if we already have it in our DAG.
            // Check event is not older that current genesis event timestamp.
            // Also check if we have the event's parents. In the case we do
            // not have the parents, we'll request them from the peer that has
            // sent this event to us. In case they do not reply in time, we drop
            // the event.

            // Check if the event is older than the genesis event. If so, we should
            // not include it in our Dag.
            // The genesis event marks the last time the Dag has been pruned of old
            // events. The pruning interval is defined by the days_rotation field
            // of [`EventGraph`].
            if event.header.timestamp < genesis_timestamp {
                debug!(
                    target: "event_graph::protocol::handle_event_put",
                    "Event {} is older than genesis. Event timestamp: `{}`. Genesis timestamp: `{genesis_timestamp}`",
                event.id(), event.header.timestamp
                );
            }

            // Validate the new event first. If we do not consider it valid, we
            // will just drop it and stay quiet. If the malicious threshold
            // is reached, we will stop the connection.
            if !event.validate_new() {
                self.clone().increase_malicious_count().await?;
                continue
            }

            // At this point, this is a new event to us. Let's see if we
            // have all of its parents.
            debug!(
                target: "event_graph::protocol::handle_event_put",
                "Event {event_id} is new"
            );

            let mut missing_parents = HashSet::new();
            for parent_id in event.header.parents.iter() {
                // `event.validate_new()` should have already made sure that
                // not all parents are NULL, and that there are no duplicates.
                if parent_id == &NULL_ID {
                    continue
                }

                if !self
                    .event_graph
                    .dag_store
                    .read()
                    .await
                    .get_dag(&hdr_tree_name)
                    .contains_key(parent_id.as_bytes())
                    .unwrap()
                {
                    missing_parents.insert(*parent_id);
                }
            }

            // If we have missing parents, then we have to attempt to
            // fetch them from this peer. Do this recursively until we
            // find all of them.
            if !missing_parents.is_empty() {
                // We track the received events mapped by their layer.
                // If/when we get all of them, we need to insert them in order so
                // the DAG state stays correct and unreferenced tips represent the
                // actual thing they should. If we insert them out of order, then
                // we might have wrong unreferenced tips.
                let mut received_events: BTreeMap<u64, Vec<Event>> = BTreeMap::new();
                let mut received_events_hashes = HashSet::new();

                debug!(
                    target: "event_graph::protocol::handle_event_put",
                    "Event has {} missing parents. Requesting...", missing_parents.len(),
                );

                let current_genesis = self.event_graph.current_genesis.read().await;
                let dag_name = current_genesis.header.timestamp.to_string();
                let hdr_tree_name = format!("headers_{dag_name}");

                while !missing_parents.is_empty() {
                    // for parent_id in missing_parents.clone().iter() {
                    debug!(
                        target: "event_graph::protocol::handle_event_put",
                        "Requesting {missing_parents:?}..."
                    );

                    self.channel
                        .send(&EventReq(missing_parents.clone().into_iter().collect()))
                        .await?;

                    // Node waits for response
                    let Ok(parents) = self
                        .ev_rep_sub
                        .receive_with_timeout(
                            self.event_graph
                                .p2p
                                .settings()
                                .read()
                                .await
                                .outbound_connect_timeout_max(),
                        )
                        .await
                    else {
                        error!(
                            target: "event_graph::protocol::handle_event_put",
                            "[EVENTGRAPH] Timeout while waiting for parents {missing_parents:?} from {}",
                            self.channel.display_address(),
                        );
                        self.channel.stop().await;
                        return Err(Error::ChannelStopped)
                    };

                    let parents = parents.0.clone();

                    for parent in parents {
                        let parent_id = parent.id();
                        if !missing_parents.contains(&parent_id) {
                            error!(
                                target: "event_graph::protocol::handle_event_put",
                                "[EVENTGRAPH] Peer {} replied with a wrong event: {}",
                                self.channel.display_address(), parent.id(),
                            );
                            self.channel.stop().await;
                            return Err(Error::ChannelStopped)
                        }

                        debug!(
                            target: "event_graph::protocol::handle_event_put",
                            "Got correct parent event {}", parent.id(),
                        );

                        if let Some(layer_events) = received_events.get_mut(&parent.header.layer) {
                            layer_events.push(parent.clone());
                        } else {
                            let layer_events = vec![parent.clone()];
                            received_events.insert(parent.header.layer, layer_events);
                        }
                        received_events_hashes.insert(parent_id);

                        missing_parents.remove(&parent_id);

                        // See if we have the upper parents
                        for upper_parent in parent.header.parents.iter() {
                            if upper_parent == &NULL_ID {
                                continue
                            }

                            if !missing_parents.contains(upper_parent) &&
                                !received_events_hashes.contains(upper_parent) &&
                                !self
                                    .event_graph
                                    .dag_store
                                    .read()
                                    .await
                                    .get_dag(&hdr_tree_name)
                                    .contains_key(upper_parent.as_bytes())
                                    .unwrap()
                            {
                                debug!(
                                    target: "event_graph::protocol::handle_event_put",
                                    "Found upper missing parent event {upper_parent}"
                                );
                                missing_parents.insert(*upper_parent);
                            }
                        }
                    }
                } // <-- while !missing_parents.is_empty()

                // At this point we should've got all the events.
                // We should add them to the DAG.
                let mut events = vec![];
                for (_, tips) in received_events {
                    for tip in tips {
                        events.push(tip);
                    }
                }
                let headers = events.iter().map(|x| x.header.clone()).collect();
                if self.event_graph.header_dag_insert(headers, &dag_name).await.is_err() {
                    self.clone().increase_malicious_count().await?;
                    continue
                }
                // FIXME
                if !self.event_graph.fast_mode {
                    if self.event_graph.dag_insert(&events, &dag_name).await.is_err() {
                        self.clone().increase_malicious_count().await?;
                        continue
                    }
                }
            } // <-- !missing_parents.is_empty()

            // If we're here, we have all the parents, and we can now
            // perform a full validation and add the actual event to
            // the DAG.
            debug!(
                target: "event_graph::protocol::handle_event_put",
                "Got all parents necessary for insertion",
            );
            if self
                .event_graph
                .header_dag_insert(vec![event.header.clone()], &dag_name)
                .await
                .is_err()
            {
                self.clone().increase_malicious_count().await?;
                continue
            }

            if self.event_graph.dag_insert(slice::from_ref(&event), &dag_name).await.is_err() {
                self.clone().increase_malicious_count().await?;
                continue
            }

            self.broadcaster_push
                .send(EventPut(event, blob))
                .await
                .expect("push broadcaster closed");
        }
    }

    async fn handle_static_put(self: Arc<Self>) -> Result<()> {
        // Rolling window of event timestamps on this channel
        let mut bantimes = MovingWindow::new(WINDOW_EXPIRY_TIME);

        loop {
            let (event, blob) = match self.st_put_sub.receive().await {
                Ok(v) => (v.0.clone(), v.1.clone()),
                Err(_) => continue,
            };
            trace!(
                 target: "event_graph::protocol::handle_static_put()",
                 "Got StaticPut: {} [{}]", event.id(), self.channel.address(),
            );

            // Check if node has finished syncing its DAG
            if !*self.event_graph.synced.read().await {
                debug!(
                    target: "event_graph::protocol::handle_static_put",
                    "DAG is still syncing, skipping..."
                );
                continue
            }

            let event_id = event.id();
            if self.event_graph.static_dag.contains_key(event_id.as_bytes())? {
                debug!(
                    target: "event_graph::protocol::handle_static_put()",
                    "Event {} is already known", event_id,
                );
                continue
            }

            // Register
            if !blob.is_empty() && !event.content().is_empty() {
                let (proof, user_msg_limit): (Proof, u64) = match deserialize_async_partial(&blob)
                    .await
                {
                    Ok((v, _)) => v,
                    Err(e) => {
                        error!(target: "event_graph::protocol::handle_static_put()","[RLN] Failed deserializing event ephemeral data: {}", e);
                        continue
                    }
                };
                let commitment: Fp = match deserialize_async_partial(&event.content()).await {
                    Ok((v, _)) => v,
                    Err(e) => {
                        error!(target: "event_graph::protocol::handle_static_put()","[EVENTGRAPH] Failed deserializing event ephemeral data: {}", e);
                        continue
                    }
                };
                info!("registering account: {}", commitment.to_string());
                let public_inputs = vec![commitment, user_msg_limit.into()];

                if proof.verify(&self.event_graph.register_vk, &public_inputs).is_err() {
                    error!(target: "event_graph::protocol::handle_static_put()", "[RLN] Incoming Event RLN Registration proof verification failed");
                    continue
                }
            }

            // Slash
            if event.content() == vec![0, 1] {
                let (proof, secret, user_msg_limit, identity_root): (
                    Proof,
                    pallas::Base,
                    u64,
                    pallas::Base,
                ) = match deserialize_async_partial(&blob).await {
                    Ok((v, _)) => v,
                    Err(e) => {
                        error!(target: "event_graph::protocol::handle_static_put()","[RLN] Failed deserializing event ephemeral data: {}", e);
                        continue
                    }
                };

                let public_inputs = vec![secret, pallas::Base::from(user_msg_limit), identity_root];

                if proof.verify(&self.event_graph.slash_vk, &public_inputs).is_err() {
                    error!(target: "event_graph::protocol::handle_static_put()", "[RLN] Incoming Event RLN Slashing proof verification failed");
                    continue
                }

                let identity_secret_hash = poseidon_hash([secret, user_msg_limit.into()]);
                let commitment = poseidon_hash([identity_secret_hash]);
                info!("slashing account: {}", commitment.to_string());
                let commitment = vec![commitment];
                let commitment: Vec<_> = commitment.into_iter().map(|l| (l, l)).collect();

                let mut rln_id_tree = self.event_graph.rln_identity_tree.write().await;
                rln_id_tree.remove_leaves(commitment)?;
            }

            // Check if event's parents are in the static DAG
            for parent in event.header.parents.iter() {
                if *parent == NULL_ID {
                    continue
                }
                if !self.event_graph.static_dag.contains_key(parent.as_bytes())? {
                    debug!(
                        target: "event_graph::protocol::handle_static_put()",
                        "Event {} is orphan", event_id,
                    );
                    return Err(Error::EventNotFound("Event is orphan".to_owned()))
                }
            }

            // There's a new unique event.
            // Apply ban logic to stop network floods.
            bantimes.ticktock();
            if bantimes.count() > WINDOW_MAXSIZE {
                self.channel.ban().await;
                // This error is actually unused. We could return Ok here too.
                return Err(Error::MaliciousFlood)
            }

            // Validate the new event first. If we do not consider it valid, we
            // will just drop it and stay quiet. If the malicious threshold
            // is reached, we will stop the connection.
            if !event.validate_new() {
                self.clone().increase_malicious_count().await?;
                continue
            }

            // At this point, this is a new event to us. Let's see if we
            // have all of its parents.
            debug!(
                target: "event_graph::protocol::handle_event_put()",
                "Event {} is new", event_id,
            );

            self.event_graph.static_insert(&event).await?;
            self.event_graph.static_broadcast(event, blob).await?
        }
    }

    /// Protocol function handling `EventReq`.
    /// This is triggered whenever someone requests an event from us.
    async fn handle_event_req(self: Arc<Self>) -> Result<()> {
        loop {
            let event_ids = match self.ev_req_sub.receive().await {
                Ok(v) => v.0.clone(),
                Err(_) => continue,
            };
            trace!(
                target: "event_graph::protocol::handle_event_req",
                "Got EventReq: {event_ids:?} [{}]", self.channel.display_address(),
            );

            // Check if node has finished syncing its DAG
            if !*self.event_graph.synced.read().await {
                debug!(
                    target: "event_graph::protocol::handle_event_req",
                    "DAG is still syncing, skipping..."
                );
                continue
            }

            // We received an event request from somebody.
            // If we do have it, we will send it back to them as `EventRep`.
            // Otherwise, we'll stay quiet. An honest node should always have
            // something to reply with provided that the request is legitimate,
            // i.e. we've sent something to them and they did not haveinfo some of
            // the parents.

            // Check if we expected this request to come around.
            // I dunno if this is a good idea, but it seems it will help
            // against malicious event requests where they want us to keep
            // reading our db and steal our bandwidth.
            let mut events = vec![];
            for event_id in event_ids.iter() {
                if let Ok(event) = self
                    .event_graph
                    .fetch_event_from_dags(event_id)
                    .await?
                    .ok_or(Error::EventNotFound("The requested event is not found".to_owned()))
                {
                    // At this point we should have it in our DAG.
                    // This code panics if this is not the case.
                    debug!(
                        target: "event_graph::protocol::handle_event_req()",
                        "Fetching event {:?} from DAG", event_id,
                    );
                    events.push(event);
                } else {
                    let malicious_count = self.malicious_count.fetch_add(1, SeqCst);
                    if malicious_count + 1 == MALICIOUS_THRESHOLD {
                        error!(
                            target: "event_graph::protocol::handle_event_req",
                            "[EVENTGRAPH] Peer {} reached malicious threshold. Dropping connection.",
                            self.channel.display_address(),
                        );
                        self.channel.stop().await;
                        return Err(Error::ChannelStopped)
                    }

                    warn!(
                        target: "event_graph::protocol::handle_event_req",
                        "[EVENTGRAPH] Peer {} requested an unexpected event {event_id:?}",
                        self.channel.display_address()
                    );
                    continue
                }
            }

            // Check if the incoming event is older than the genesis event. If so, something
            // has gone wrong. The event should have been pruned during the last
            // rotation.
            let genesis_timestamp = self.event_graph.current_genesis.read().await.header.timestamp;
            let mut bcast_ids = self.event_graph.broadcasted_ids.write().await;

            for event in events.iter() {
                if event.header.timestamp < genesis_timestamp {
                    error!(
                        target: "event_graph::protocol::handle_event_req",
                        "Requested event by peer {} is older than previous rotation period. It should have been pruned.
                    Event timestamp: `{}`. Genesis timestamp: `{genesis_timestamp}`",
                    event.id(), event.header.timestamp
                    );
                }

                // Now let's get the upper level of event IDs. When we reply, we could
                // get requests for those IDs as well.
                for parent_id in event.header.parents.iter() {
                    if parent_id != &NULL_ID {
                        bcast_ids.insert(*parent_id);
                    }
                }
            }
            // TODO: We should remove the reply from the bcast IDs for this specific channel.
            //       We can't remove them for everyone.
            //bcast_ids.remove(&event_id);
            drop(bcast_ids);

            // Reply with the event
            self.channel.send(&EventRep(events)).await?;
        }
    }

    /// Protocol function handling `HeaderReq`.
    /// This is triggered whenever someone requests syncing headers by
    /// sending their current headers.
    async fn handle_header_req(self: Arc<Self>) -> Result<()> {
        loop {
            let dag_name = match self.hdr_req_sub.receive().await {
                Ok(v) => v.0.clone(),
                Err(_) => continue,
            };
            trace!(
                target: "event_graph::protocol::handle_tip_req",
                "Got TipReq [{}]", self.channel.display_address(),
            );

            // Check if node has finished syncing its DAG
            if !*self.event_graph.synced.read().await {
                debug!(
                    target: "event_graph::protocol::handle_tip_req",
                    "DAG is still syncing, skipping..."
                );
                continue
            }

            // TODO: Rate limit

            // We received header request. Let's find them, add them to
            // our bcast ids list, and reply with them.
            let dag_timestamp = u64::from_str(&dag_name)?;
            let store = self.event_graph.dag_store.read().await;
            if !store.header_dags.contains_key(&dag_timestamp) {
                continue
            }
            let main_dag = store.get_dag(&dag_name);
            let mut headers = vec![];
            for item in main_dag.iter() {
                let (_, event) = item.unwrap();
                let event: Event = deserialize_async(&event).await.unwrap();
                if !headers.contains(&event.header) || event.header.parents != NULL_PARENTS {
                    headers.push(event.header);
                }
            }
            // let mut bcast_ids = self.event_graph.broadcasted_ids.write().await;
            // for (_, tips) in layers.iter() {
            //     for tip in tips {
            //         bcast_ids.insert(*tip);
            //     }
            // }
            // drop(bcast_ids);

            self.channel.send(&HeaderRep(headers)).await?;
        }
        // Ok(())
    }

    /// Protocol function handling `TipReq`.
    /// This is triggered when someone requests the current unreferenced
    /// tips of our DAG.
    async fn handle_tip_req(self: Arc<Self>) -> Result<()> {
        loop {
            let dag_name = match self.tip_req_sub.receive().await {
                Ok(v) => v.0.clone(),
                Err(_) => continue,
            };
            trace!(
                target: "event_graph::protocol::handle_tip_req",
                "Got TipReq [{}]", self.channel.display_address(),
            );

            // Check if node has finished syncing its DAG
            if !*self.event_graph.synced.read().await {
                debug!(
                    target: "event_graph::protocol::handle_tip_req",
                    "DAG is still syncing, skipping..."
                );
                continue
            }

            // TODO: Rate limit

            // We received a tip request. Let's find them, add them to
            // our bcast ids list, and reply with them.
            let layers = match dag_name.as_str() {
                "static-dag" => {
                    let tips = self.event_graph.static_unreferenced_tips().await;
                    &tips.clone()
                }
                _ => {
                    let dag_timestamp = u64::from_str(&dag_name)?;
                    let store = self.event_graph.dag_store.read().await;
                    let (_, layers) = match store.header_dags.get(&dag_timestamp) {
                        Some(v) => v,
                        None => continue,
                    };
                    &layers.clone()
                }
            };
            // let layers = self.event_graph.dag_store.read().await.find_unreferenced_tips(&dag_name).await;
            let mut bcast_ids = self.event_graph.broadcasted_ids.write().await;
            for (_, tips) in layers.iter() {
                for tip in tips {
                    bcast_ids.insert(*tip);
                }
            }
            drop(bcast_ids);

            self.channel.send(&TipRep(layers.clone())).await?;
        }
    }

    /// We need to rate limit message propagation so malicious nodes don't get us banned
    /// for flooding. We do that by aggregating messages here into a queue then apply
    /// rate limit logic before broadcasting.
    ///
    /// The rate limit logic is this:
    ///
    /// * If the count is less then RATELIMIT_MIN_COUNT then do nothing.
    /// * Otherwise sleep for `sleep_time` ms.
    ///
    /// To calculate the sleep time, we use the RATELIMIT_SAMPLE_* values.
    /// For example RATELIMIT_SAMPLE_IDX = 10, RATELIMIT_SAMPLE_SLEEP = 1000
    /// means that when N = 10, then sleep for 1000 ms.
    ///
    /// Let RATELIMIT_MIN_COUNT = 6, then here's a table of sleep times:
    ///
    /// | Count | Sleep Time / ms |
    /// |-------|-----------------|
    /// | 0     | 0               |
    /// | 4     | 0               |
    /// | 6     | 0               |
    /// | 10    | 1000            |
    /// | 14    | 2000            |
    /// | 18    | 3000            |
    ///
    /// So we use the sample to calculate a straight line from RATELIMIT_MIN_COUNT.
    async fn broadcast_rate_limiter(self: Arc<Self>) -> Result<()> {
        let mut ratelimit = MovingWindow::new(RATELIMIT_EXPIRY_TIME);

        loop {
            let event_put = self.broadcaster_pull.recv().await.expect("pull broadcaster closed");

            ratelimit.ticktock();
            if ratelimit.count() > RATELIMIT_MIN_COUNT {
                let sleep_time =
                    ((ratelimit.count() - RATELIMIT_MIN_COUNT) * RATELIMIT_SAMPLE_SLEEP /
                        (RATELIMIT_SAMPLE_IDX - RATELIMIT_MIN_COUNT)) as u64;
                debug!(
                    target: "event_graph::protocol::broadcast_rate_limiter",
                    "Activated rate limit: sleeping {sleep_time} ms [count={}]",
                    ratelimit.count()
                );
                // Apply the ratelimit
                msleep(sleep_time).await;
            }

            // Relay the event to other peers.
            self.event_graph
                .p2p
                .broadcast_with_exclude(&event_put, &[self.channel.address().clone()])
                .await;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::time::UNIX_EPOCH;

    #[test]
    fn test_eventgraph_moving_window_clean_future() {
        let mut window = MovingWindow::new(NanoTimestamp::from_secs(60));
        let future = UNIX_EPOCH.elapsed().unwrap().as_secs() + 100;
        window.times.push_back(NanoTimestamp::from_secs(future.into()));
        window.clean();
        assert_eq!(window.count(), 0);
    }
}
