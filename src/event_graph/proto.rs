/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

// TODO: FIXME: Some of the protocols should block operations until DAG is synced.

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering::SeqCst},
        Arc,
    },
    time::Duration,
};

use darkfi_serial::{async_trait, deserialize_async, SerialDecodable, SerialEncodable};
use log::{debug, error, trace, warn};
use smol::Executor;

use super::{Event, EventGraphPtr, NULL_ID};
use crate::{impl_p2p_message, net::*, system::timeout::timeout, Error, Result};

/// Malicious behaviour threshold. If the threshold is reached, we will
/// drop the peer from our P2P connection.
const MALICIOUS_THRESHOLD: usize = 5;
/// Time to wait for a parent ID reply
pub(super) const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// P2P protocol implementation for the Event Graph.
pub struct ProtocolEventGraph {
    /// Pointer to the connected peer
    channel: ChannelPtr,
    /// Pointer to the Event Graph instance
    event_graph: EventGraphPtr,
    /// `MessageSubscriber` for `EventPut`
    ev_put_sub: MessageSubscription<EventPut>,
    /// `MessageSubscriber` for `EventReq`
    ev_req_sub: MessageSubscription<EventReq>,
    /// `MessageSubscriber` for `EventRep`
    ev_rep_sub: MessageSubscription<EventRep>,
    /// `MessageSubscriber` for `TipReq`
    tip_req_sub: MessageSubscription<TipReq>,
    /// `MessageSubscriber` for `TipRep`
    _tip_rep_sub: MessageSubscription<TipRep>,
    /// Peer malicious message count
    malicious_count: AtomicUsize,
    /// P2P jobs manager pointer
    jobsman: ProtocolJobsManagerPtr,
}

/// A P2P message representing publishing an event on the network
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct EventPut(pub Event);
impl_p2p_message!(EventPut, "EventGraph::EventPut");

/// A P2P message representing an event request
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct EventReq(pub blake3::Hash);
impl_p2p_message!(EventReq, "EventGraph::EventReq");

/// A P2P message representing an event reply
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct EventRep(pub Event);
impl_p2p_message!(EventRep, "EventGraph::EventRep");

/// A P2P message representing a request for a peer's DAG tips
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct TipReq {}
impl_p2p_message!(TipReq, "EventGraph::TipReq");

/// A P2P message representing a reply for the peer's DAG tips
#[derive(Clone, SerialEncodable, SerialDecodable)]
pub struct TipRep(pub Vec<blake3::Hash>);
impl_p2p_message!(TipRep, "EventGraph::TipRep");

#[async_trait]
impl ProtocolBase for ProtocolEventGraph {
    async fn start(self: Arc<Self>, ex: Arc<Executor<'_>>) -> Result<()> {
        self.jobsman.clone().start(ex.clone());
        self.jobsman.clone().spawn(self.clone().handle_event_put(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_event_req(), ex.clone()).await;
        self.jobsman.clone().spawn(self.clone().handle_tip_req(), ex.clone()).await;
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
        msg_subsystem.add_dispatch::<EventReq>().await;
        msg_subsystem.add_dispatch::<EventRep>().await;
        msg_subsystem.add_dispatch::<TipReq>().await;
        msg_subsystem.add_dispatch::<TipRep>().await;

        let ev_put_sub = channel.subscribe_msg::<EventPut>().await?;
        let ev_req_sub = channel.subscribe_msg::<EventReq>().await?;
        let ev_rep_sub = channel.subscribe_msg::<EventRep>().await?;
        let tip_req_sub = channel.subscribe_msg::<TipReq>().await?;
        let _tip_rep_sub = channel.subscribe_msg::<TipRep>().await?;

        Ok(Arc::new(Self {
            channel: channel.clone(),
            event_graph,
            ev_put_sub,
            ev_req_sub,
            ev_rep_sub,
            tip_req_sub,
            _tip_rep_sub,
            malicious_count: AtomicUsize::new(0),
            jobsman: ProtocolJobsManager::new("ProtocolEventGraph", channel.clone()),
        }))
    }

    /// Protocol function handling `EventPut`.
    /// This is triggered whenever someone broadcasts (or relays) a new
    /// event on the network.
    async fn handle_event_put(self: Arc<Self>) -> Result<()> {
        loop {
            let event = match self.ev_put_sub.receive().await {
                Ok(v) => v.0.clone(),
                Err(_) => continue,
            };
            trace!(
                 target: "event_graph::protocol::handle_event_put()",
                 "Got EventPut: {} [{}]", event.id(), self.channel.address(),
            );

            // We received an event. Check if we already have it in our DAG.
            // Also check if we have the event's parents. In the case we do
            // not have the parents, we'll request them from the peer that has
            // sent this event to us. In case they do not reply in time, we drop
            // the event.

            // Validate the event first. If we do not consider it valid, we
            // will just drop it and stay quiet. If the malicious threshold
            // is reached, we will stop the connection.
            if !event.validate() {
                let malicious_count = self.malicious_count.fetch_add(1, SeqCst);
                if malicious_count + 1 == MALICIOUS_THRESHOLD {
                    error!(
                        target: "event_graph::protocol::handle_event_put()",
                        "[EVENTGRAPH] Peer {} reached malicious threshold. Dropping connection.",
                        self.channel.address(),
                    );
                    self.channel.stop().await;
                    return Err(Error::ChannelStopped)
                }

                warn!(
                    target: "event_graph::protocol::handle_event_put()",
                    "[EVENTGRAPH] Peer {} sent us a malicious event", self.channel.address(),
                );
                continue
            }

            // If we have already seen the event, we'll stay quiet.
            let event_id = event.id();
            if self.event_graph.dag.contains_key(event_id.as_bytes()).unwrap() {
                debug!(
                    target: "event_graph::protocol::handle_event_put()",
                    "Event {} is already known", event_id,
                );
                continue
            }

            // At this point, this is a new event to us. Let's see if we
            // have all of its parents.
            debug!(
                target: "event_graph::protocol::handle_event_put()",
                "Event {} is new", event_id,
            );

            let mut missing_parents = vec![];
            for parent_id in event.parents.iter() {
                // `event.validate()` should have already made sure that
                // not all parents are NULL, and that there are no duplicates.
                if parent_id == &NULL_ID {
                    continue
                }

                if !self.event_graph.dag.contains_key(parent_id.as_bytes()).unwrap() {
                    missing_parents.push(*parent_id);
                }
            }

            // If we have missing parents, then we have to attempt to
            // fetch them from this peer. Do this recursively until we
            // find all of them.
            if !missing_parents.is_empty() {
                // We track the received events in a vec. If/when we get all
                // of them, we need to insert them in reverse so the DAG state
                // stays correct and unreferenced tips represent the actual thing
                // they should. If we insert them out of order, then we might have
                // wrong unreferenced tips.
                // TODO: What should we do if at some point the events become too old?
                let mut received_events = vec![];

                debug!(
                    target: "event_graph::protocol::handle_event_put()",
                    "Event has {} missing parents. Requesting...", missing_parents.len(),
                );

                while !missing_parents.is_empty() {
                    for parent_id in missing_parents.clone().iter() {
                        debug!(
                            target: "event_graph::protocol::handle_event_put()",
                            "Requesting {}...", parent_id,
                        );

                        self.channel.send(&EventReq(*parent_id)).await?;
                        let parent = match timeout(REPLY_TIMEOUT, self.ev_rep_sub.receive()).await {
                            Ok(parent) => parent?,
                            Err(_) => {
                                error!(
                                    target: "event_graph::protocol::handle_event_put()",
                                    "[EVENTGRAPH] Timeout while waiting for parent {} from {}",
                                    parent_id, self.channel.address(),
                                );
                                self.channel.stop().await;
                                return Err(Error::ChannelStopped)
                            }
                        };
                        let parent = parent.0.clone();

                        if &parent.id() != parent_id {
                            error!(
                                target: "event_graph::protocol::handle_event_put()",
                                "[EVENTGRAPH] Peer {} replied with a wrong event: {}",
                                self.channel.address(), parent.id(),
                            );
                            self.channel.stop().await;
                            return Err(Error::ChannelStopped)
                        }

                        debug!(
                            target: "event_graph::protocol::handle_event_put()",
                            "Got correct parent event {}", parent.id(),
                        );

                        received_events.push(parent.clone());
                        let pos = missing_parents.iter().position(|id| id == &parent.id()).unwrap();
                        missing_parents.remove(pos);

                        // See if we have the upper parents
                        for upper_parent in parent.parents.iter() {
                            if upper_parent == &NULL_ID {
                                continue
                            }

                            if !self.event_graph.dag.contains_key(upper_parent.as_bytes()).unwrap()
                            {
                                debug!(
                                    target: "event_graph::protocol::handle_event_put()",
                                    "Found upper missing parent event{}", upper_parent,
                                );
                                missing_parents.push(*upper_parent);
                            }
                        }
                    }
                } // <-- while !missing_parents.is_empty()

                // At this point we should've got all the events.
                // We should add them to the DAG.
                // TODO: FIXME: Also validate these events.
                for event in received_events.iter().rev() {
                    self.event_graph.dag_insert(event.clone()).await.unwrap();
                }
            } // <-- !missing_parents.is_empty()

            // If we're here, we have all the parents, and we can now
            // add the actual event to the DAG.
            debug!(
                target: "event_graph::protocol::handle_event_put()",
                "Got all parents necessary for insertion",
            );
            self.event_graph.dag_insert(event.clone()).await.unwrap();

            // Relay the event to other peers.
            self.event_graph
                .p2p
                .broadcast_with_exclude(&EventPut(event), &[self.channel.address().clone()])
                .await;
        }
    }

    /// Protocol function handling `EventReq`.
    /// This is triggered whenever someone requests an event from us.
    async fn handle_event_req(self: Arc<Self>) -> Result<()> {
        loop {
            let event_id = match self.ev_req_sub.receive().await {
                Ok(v) => v.0,
                Err(_) => continue,
            };
            trace!(
                target: "event_graph::protocol::handle_event_req()",
                "Got EventReq: {} [{}]", event_id, self.channel.address(),
            );

            // We received an event request from somebody.
            // If we do have it, we will send it back to them as `EventRep`.
            // Otherwise, we'll stay quiet. An honest node should always have
            // something to reply with provided that the request is legitimate,
            // i.e. we've sent something to them and they did not have some of
            // the parents.

            // Check if we expected this request to come around.
            // I dunno if this is a good idea, but it seems it will help
            // against malicious event requests where they want us to keep
            // reading our db and steal our bandwidth.
            if !self.event_graph.broadcasted_ids.read().await.contains(&event_id) {
                let malicious_count = self.malicious_count.fetch_add(1, SeqCst);
                if malicious_count + 1 == MALICIOUS_THRESHOLD {
                    error!(
                        target: "event_graph::protocol::handle_event_req()",
                        "[EVENTGRAPH] Peer {} reached malicious threshold. Dropping connection.",
                        self.channel.address(),
                    );
                    self.channel.stop().await;
                    return Err(Error::ChannelStopped)
                }

                warn!(
                    target: "event_graph::protocol::handle_event_req()",
                    "[EVENTGRAPH] Peer {} requested an unexpected event {}",
                    self.channel.address(), event_id,
                );
                continue
            }

            // At this point we should have it in our DAG.
            // This code panics if this is not the case.
            let event = self.event_graph.dag.get(event_id.as_bytes()).unwrap().unwrap();
            let event: Event = deserialize_async(&event).await.unwrap();

            // Now let's get the upper level of event IDs. When we reply, we could
            // get requests for those IDs as well.
            let mut bcast_ids = self.event_graph.broadcasted_ids.write().await;
            for parent_id in event.parents.iter() {
                if parent_id != &NULL_ID {
                    bcast_ids.insert(*parent_id);
                }
            }

            // TODO: We should remove the reply from the bcast IDs for this specific channel.
            //       We can't remove them for everyone.
            //bcast_ids.remove(&event_id);
            drop(bcast_ids);

            // Reply with the event
            self.channel.send(&EventRep(event)).await?;
        }
    }

    /// Protocol function handling `TipReq`.
    /// This is triggered when someone requests the current unreferenced
    /// tips of our DAG.
    async fn handle_tip_req(self: Arc<Self>) -> Result<()> {
        loop {
            self.tip_req_sub.receive().await?;
            trace!(
                target: "event_graph::protocol::handle_tip_req()",
                "Got TipReq [{}]", self.channel.address(),
            );

            // TODO: Rate limit

            // We received a tip request. Let's find them, add them to
            // our bcast ids list, and reply with them.
            let mut tips = self.event_graph.get_unreferenced_tips().await.to_vec();
            tips.retain(|x| x != &NULL_ID);

            let mut bcast_ids = self.event_graph.broadcasted_ids.write().await;
            for tip in tips.iter() {
                bcast_ids.insert(*tip);
            }
            drop(bcast_ids);

            self.channel.send(&TipRep(tips)).await?;
        }
    }
}
