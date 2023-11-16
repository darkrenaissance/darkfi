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

use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::UNIX_EPOCH,
};

use async_recursion::async_recursion;
use darkfi_serial::{deserialize_async, serialize_async};
use log::{debug, error, info};
use num_bigint::BigUint;
use smol::{
    lock::{OnceCell, RwLock},
    Executor,
};

use crate::{
    net::P2pPtr,
    system::{sleep, timeout::timeout, StoppableTask, StoppableTaskPtr, Subscriber, SubscriberPtr},
    Error, Result,
};

/// An event graph event
pub mod event;
pub use event::Event;

/// P2P protocol implementation for the Event Graph
pub mod proto;
use proto::{EventRep, EventReq, TipRep, TipReq, REPLY_TIMEOUT};

/// Utility functions
mod util;
use util::{days_since, next_rotation_timestamp, DAY};

#[cfg(test)]
mod tests;

/// Initial genesis timestamp (07 Sep 2023, 00:00:00 UTC)
/// Must always be UTC midnight.
const INITIAL_GENESIS: u64 = 1694044800;
/// Genesis event contents
const GENESIS_CONTENTS: &[u8] = &[0x47, 0x45, 0x4e, 0x45, 0x53, 0x49, 0x53];

/// The number of parents an event is supposed to have.
const N_EVENT_PARENTS: usize = 5;
/// Allowed timestamp drift in seconds
const EVENT_TIME_DRIFT: u64 = 60;
/// Null event ID
pub const NULL_ID: blake3::Hash = blake3::Hash::from_bytes([0x00; blake3::OUT_LEN]);

/// Atomic pointer to an [`EventGraph`] instance.
pub type EventGraphPtr = Arc<EventGraph>;

/// An Event Graph instance
pub struct EventGraph {
    /// Pointer to the P2P network instance
    p2p: P2pPtr,
    /// Sled tree containing the DAG
    dag: sled::Tree,
    /// The set of unreferenced DAG tips
    unreferenced_tips: RwLock<HashSet<blake3::Hash>>,
    /// A `HashSet` containg event IDs and their 1-level parents.
    /// These come from the events we've sent out using `EventPut`.
    /// They are used with `EventReq` to decide if we should reply
    /// or not. Additionally it is also used when we broadcast the
    /// `TipRep` message telling peers about our unreferenced tips.
    broadcasted_ids: RwLock<HashSet<blake3::Hash>>,
    /// DAG Pruning Task
    prune_task: OnceCell<StoppableTaskPtr>,
    /// Event subscriber, this notifies whenever an event is
    /// inserted into the DAG
    pub event_sub: SubscriberPtr<Event>,
}

impl EventGraph {
    /// Create a new [`EventGraph`] instance.
    /// * `days_rotation` marks the lifetime of the DAG before it's pruned.
    pub async fn new(
        p2p: P2pPtr,
        sled_db: sled::Db,
        dag_tree_name: &str,
        days_rotation: u64,
        ex: Arc<Executor<'_>>,
    ) -> Result<EventGraphPtr> {
        let dag = sled_db.open_tree(dag_tree_name)?;
        let unreferenced_tips = RwLock::new(HashSet::new());
        let broadcasted_ids = RwLock::new(HashSet::new());
        let event_sub = Subscriber::new();

        let self_ = Arc::new(Self {
            p2p,
            dag: dag.clone(),
            unreferenced_tips,
            broadcasted_ids,
            prune_task: OnceCell::new(),
            event_sub,
        });

        // Create the current genesis event based on the `days_rotation`
        let current_genesis = Self::generate_genesis(days_rotation);

        // Check if we have it in our DAG.
        // If not, we can prune the DAG and insert this new genesis event.
        if !dag.contains_key(current_genesis.id().as_bytes())? {
            info!(
                target: "event_graph::new()",
                "[EVENTGRAPH] DAG does not contain current genesis, pruning existing data",
            );
            self_.dag_prune(current_genesis).await?;
        }

        // Find the unreferenced tips in the current DAG state.
        *self_.unreferenced_tips.write().await = self_.find_unreferenced_tips().await;

        // Spawn the DAG pruning task
        if days_rotation > 0 {
            let self__ = self_.clone();
            let prune_task = StoppableTask::new();
            let _ = self_.prune_task.set(prune_task.clone()).await;

            prune_task.clone().start(
                self_.clone().dag_prune_task(days_rotation),
                |_| async move {
                    self__.clone()._handle_stop(sled_db).await;
                },
                Error::DetachedTaskStopped,
                ex.clone(),
            );
        }

        Ok(self_)
    }

    async fn _handle_stop(&self, sled_db: sled::Db) {
        info!(target: "event_graph::_handle_stop()", "[EVENTGRAPH] Prune task stopped, flushing sled");
        sled_db.flush_async().await.unwrap();
    }

    /// Generate a deterministic genesis event corresponding to the DAG's configuration.
    fn generate_genesis(days_rotation: u64) -> Event {
        // Days rotation is u64 except zero
        let genesis_days_rotation = if days_rotation == 0 { 1 } else { days_rotation };

        // First check how many days passed since initial genesis.
        let days_passed = days_since(INITIAL_GENESIS);

        // Calculate the number of days_rotation intervals since INITIAL_GENESIS
        let rotations_since_genesis = days_passed / genesis_days_rotation;

        // Calculate the timestamp of the most recent event
        let timestamp =
            INITIAL_GENESIS + (rotations_since_genesis * genesis_days_rotation * DAY as u64);

        Event { timestamp, content: GENESIS_CONTENTS.to_vec(), parents: [NULL_ID; N_EVENT_PARENTS] }
    }

    /// Sync the DAG from connected peers
    pub async fn dag_sync(&self) -> Result<()> {
        // We do an optimistic sync where we ask all our connected peers for
        // the DAG tips (unreferenced events)  and then we accept the ones we
        // see the most times.
        // * Compare received tips with local ones, identify which we are missing.
        // * Request these from peers
        // * Recursively request these backward
        //
        // Verification:
        // * Timestamps should go backwards
        // * Cross-check with multiple peers, this means we should request the
        //   same event from multiple peers and make sure it is the same.
        // * Since we should be pruning, if we're not synced after some reasonable
        //   amount of iterations, these could be faulty peers and we can try again
        //   from the beginning

        // Get references to all our peers.
        let channels = self.p2p.channels().await;
        let mut communicated_peers = channels.len();
        info!(
            target: "event_graph::dag_sync()",
            "[EVENTGRAPH] Syncing DAG from {} peers...", communicated_peers,
        );

        // Here we keep track of the tips and how many time we've seen them.
        let mut tips: HashMap<blake3::Hash, usize> = HashMap::new();

        // Let's first ask all of our peers for their tips and collect them
        // in our hashmap above.
        for channel in channels.iter() {
            let url = channel.address();

            let tip_rep_sub = match channel.subscribe_msg::<TipRep>().await {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "event_graph::dag_sync()",
                        "[EVENTGRAPH] Sync: Couldn't subscribe TipReq for peer {}, skipping ({})",
                        url, e,
                    );
                    communicated_peers -= 1;
                    continue
                }
            };

            if let Err(e) = channel.send(&TipReq {}).await {
                error!(
                    target: "event_graph::dag_sync()",
                    "[EVENTGRAPH] Sync: Couldn't contact peer {}, skipping ({})", url, e,
                );
                communicated_peers -= 1;
                continue
            };

            let peer_tips = match timeout(REPLY_TIMEOUT, tip_rep_sub.receive()).await {
                Ok(peer_tips) => peer_tips?,
                Err(_) => {
                    error!(
                        target: "event_graph::dag_sync()",
                        "[EVENTGRAPH] Sync: Peer {} didn't reply with tips in time, skipping", url,
                    );
                    communicated_peers -= 1;
                    continue
                }
            };
            let peer_tips = &peer_tips.0;

            // Note down the seen tips
            for tip in peer_tips {
                if let Some(seen_tip) = tips.get_mut(tip) {
                    *seen_tip += 1;
                } else {
                    tips.insert(*tip, 1);
                }
            }
        }

        // After we've communicated all the peers, let's see what happened.
        if tips.is_empty() {
            error!(
                target: "event_graph::dag_sync()",
                "[EVENTGRAPH] Sync: Could not find any DAG tips",
            );
            return Err(Error::DagSyncFailed)
        }

        // We know the number of peers we've communicated with.
        // Arbitrarily, let's not consider events we only got once.
        // TODO: This should be more sensible depending on the peer number.
        let mut considered_tips = HashSet::new();
        for (tip, amount) in tips.iter() {
            if amount > &1 {
                considered_tips.insert(*tip);
            }
        }
        drop(tips);

        // Now begin fetching the events backwards.
        let mut missing_parents = vec![];
        for tip in considered_tips.iter() {
            assert!(tip != &NULL_ID);

            if !self.dag.contains_key(tip.as_bytes()).unwrap() {
                missing_parents.push(*tip);
            }
        }

        if missing_parents.is_empty() {
            info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] DAG synced successfully!");
            return Ok(())
        }

        info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] Fetching events");
        let mut received_events = vec![];
        while !missing_parents.is_empty() {
            for parent_id in missing_parents.clone().iter() {
                let mut found_event = false;

                for channel in channels.iter() {
                    let url = channel.address();

                    debug!(
                        target: "event_graph::dag_sync()",
                        "Requesting {} from {}...", parent_id, url,
                    );

                    let ev_rep_sub = match channel.subscribe_msg::<EventRep>().await {
                        Ok(v) => v,
                        Err(e) => {
                            error!(
                                target: "event_graph::dag_sync()",
                                "[EVENTGRAPH] Sync: Couldn't subscribe EventRep for peer {}, skipping ({})",
                                url, e,
                            );
                            continue
                        }
                    };

                    if let Err(e) = channel.send(&EventReq(*parent_id)).await {
                        error!(
                            target: "event_graph::dag_sync()",
                            "[EVENTGRAPH] Sync: Failed communicating EventReq({}) to {}: {}",
                            parent_id, url, e,
                        );
                        continue
                    }

                    let parent = match timeout(REPLY_TIMEOUT, ev_rep_sub.receive()).await {
                        Ok(parent) => parent,
                        Err(_) => {
                            error!(
                                target: "event_graph::dag_sync()",
                                "[EVENTGRAPH] Sync: Timeout waiting for parent {} from {}",
                                parent_id, url,
                            );
                            continue
                        }
                    };

                    let parent = match parent {
                        Ok(v) => v.0.clone(),
                        Err(e) => {
                            error!(
                                target: "event_graph::dag_sync()",
                                "[EVENTGRAPH] Sync: Failed receiving parent {}: {}",
                                parent_id, e,
                            );
                            continue
                        }
                    };

                    if &parent.id() != parent_id {
                        error!(
                            target: "event_graph::dag_sync()",
                            "[EVENTGRAPH] Sync: Peer {} replied with a wrong event: {}",
                            url, parent.id(),
                        );
                        continue
                    }

                    debug!(
                        target: "event_graph::dag_sync()",
                        "Got correct parent event {}", parent_id,
                    );

                    received_events.push(parent.clone());
                    let pos = missing_parents.iter().position(|id| id == &parent.id()).unwrap();
                    missing_parents.remove(pos);
                    found_event = true;

                    // See if we have the upper parents
                    for upper_parent in parent.parents.iter() {
                        if upper_parent == &NULL_ID {
                            continue
                        }

                        if !self.dag.contains_key(upper_parent.as_bytes()).unwrap() {
                            debug!(
                                target: "event_graph::dag_sync()",
                                "Found upper missing parent event{}", upper_parent,
                            );
                            missing_parents.push(*upper_parent);
                        }
                    }

                    break
                }

                if !found_event {
                    error!(
                        target: "event_graph::dag_sync()",
                        "[EVENTGRAPH] Sync: Failed to get all events",
                    );
                    return Err(Error::DagSyncFailed)
                }
            }
        } // <-- while !missing_parents.is_empty

        // At this point we should've got all the events.
        // We should add them to the DAG.
        // TODO: FIXME: Also validate these events.
        for event in received_events.iter().rev() {
            self.dag_insert(event.clone()).await.unwrap();
        }

        info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] DAG synced successfully!");
        Ok(())
    }

    /// Atomically prune the DAG and insert the given event as genesis.
    async fn dag_prune(&self, genesis_event: Event) -> Result<()> {
        debug!(target: "event_graph::dag_prune()", "Pruning DAG...");

        // Acquire exclusive locks to unreferenced_tips and broadcasted_ids while
        // this operation is happening. We do this to ensure that during the pruning
        // operation, no other operations are able to access the intermediate state
        // which could lead to producing the wrong state after pruning.
        let mut unreferenced_tips = self.unreferenced_tips.write().await;
        let mut broadcasted_ids = self.broadcasted_ids.write().await;

        // Atomically clear the DAG and write the new genesis event.
        let mut batch = sled::Batch::default();
        for key in self.dag.iter().keys() {
            batch.remove(key.unwrap());
        }
        batch.insert(genesis_event.id().as_bytes(), serialize_async(&genesis_event).await);

        debug!(target: "event_graph::dag_prune()", "Applying batch...");
        if let Err(e) = self.dag.apply_batch(batch) {
            panic!("Failed pruning DAG, sled apply_batch error: {}", e);
        }

        // Clear unreferenced tips and bcast ids
        *unreferenced_tips = HashSet::from([genesis_event.id()]);
        *broadcasted_ids = HashSet::new();
        drop(unreferenced_tips);
        drop(broadcasted_ids);

        debug!(target: "event_graph::dag_prune()", "DAG pruned successfully");
        Ok(())
    }

    /// Background task periodically pruning the DAG.
    async fn dag_prune_task(self: Arc<Self>, days_rotation: u64) -> Result<()> {
        // The DAG should periodically be pruned. This can be a configurable
        // parameter. By pruning, we should deterministically replace the
        // genesis event (can use a deterministic timestamp) and drop everything
        // in the DAG, leaving just the new genesis event.
        debug!(target: "event_graph::dag_prune_task()", "Spawned background DAG pruning task");

        loop {
            // Find the next rotation timestamp:
            let next_rotation = next_rotation_timestamp(INITIAL_GENESIS, days_rotation);

            // Prepare the new genesis event
            let current_genesis = Event {
                timestamp: next_rotation,
                content: GENESIS_CONTENTS.to_vec(),
                parents: [NULL_ID; N_EVENT_PARENTS],
            };

            // Sleep until it's time to rotate.
            let s = UNIX_EPOCH.elapsed().unwrap().as_secs() - next_rotation;
            debug!(target: "event_graph::dag_prune_task()", "Sleeping {}s until next DAG prune", s);
            sleep(s).await;
            debug!(target: "event_graph::dag_prune_task()", "Rotation period reached");

            // Trigger DAG prune
            self.dag_prune(current_genesis).await?;
        }
    }

    /// Insert an event into the DAG.
    /// This will append the new event into the unreferenced tips set, and
    /// remove the event's parents from it. It will also append the event's
    /// level-1 parents to the `broadcasted_ids` set, so the P2P protocol
    /// knows that any requests for them are actually legitimate.
    /// TODO: The `broadcasted_ids` set should periodically be pruned, when
    /// some sensible time has passed after broadcasting the event.
    pub async fn dag_insert(&self, event: Event) -> Result<blake3::Hash> {
        let event_id = event.id();
        debug!(target: "event_graph::dag_insert()", "Inserting event {} into the DAG", event_id);
        let s_event = serialize_async(&event).await;

        // Update the unreferenced DAG tips set
        let mut unreferenced_tips = self.unreferenced_tips.write().await;
        let mut bcast_ids = self.broadcasted_ids.write().await;

        for parent_id in event.parents.iter() {
            if parent_id != &NULL_ID {
                unreferenced_tips.remove(parent_id);
                bcast_ids.insert(*parent_id);
            }
        }
        unreferenced_tips.insert(event_id);

        self.dag.insert(event_id.as_bytes(), s_event).unwrap();

        // We hold the write locks until this point because we insert the event
        // into the database above, so we don't want anything to read these until
        // that insertion is complete.
        drop(unreferenced_tips);
        drop(bcast_ids);

        // Notify about the event on the event subscriber
        self.event_sub.notify(event).await;

        Ok(event_id)
    }

    /// Fetch an event from the DAG
    pub async fn dag_get(&self, event_id: &blake3::Hash) -> Result<Option<Event>> {
        let Some(bytes) = self.dag.get(event_id.as_bytes())? else { return Ok(None) };
        let event: Event = deserialize_async(&bytes).await?;

        Ok(Some(event))
    }

    /// Find the unreferenced tips in the current DAG state.
    async fn find_unreferenced_tips(&self) -> HashSet<blake3::Hash> {
        // First get all the event IDs
        let mut tips = HashSet::new();
        for iter_elem in self.dag.iter() {
            let (id, _) = iter_elem.unwrap();
            let id = blake3::Hash::from_bytes((&id as &[u8]).try_into().unwrap());
            tips.insert(id);
        }

        for iter_elem in self.dag.iter() {
            let (_, event) = iter_elem.unwrap();
            let event: Event = deserialize_async(&event).await.unwrap();
            for parent in event.parents.iter() {
                tips.remove(parent);
            }
        }

        tips
    }

    /// Get the current set of unreferenced tips in the DAG.
    async fn get_unreferenced_tips(&self) -> [blake3::Hash; N_EVENT_PARENTS] {
        // TODO: return vec of all instead of N_EVENT_PARENTS
        let unreferenced_tips = self.unreferenced_tips.read().await;

        let mut tips = [NULL_ID; N_EVENT_PARENTS];
        for (i, tip) in unreferenced_tips.iter().take(N_EVENT_PARENTS).enumerate() {
            tips[i] = *tip
        }

        assert!(tips.iter().any(|x| x != &NULL_ID));
        tips
    }

    /// Internal function used for DAG sorting.
    async fn get_unreferenced_tips_sorted(&self) -> [blake3::Hash; N_EVENT_PARENTS] {
        let tips = self.get_unreferenced_tips().await;

        // Convert the hash to BigUint for sorting
        let mut sorted: Vec<_> =
            tips.iter().map(|x| BigUint::from_bytes_be(x.as_bytes())).collect();
        sorted.sort_unstable();

        // Convert back to blake3
        let mut tips_sorted = [NULL_ID; N_EVENT_PARENTS];
        for (i, id) in sorted.iter().enumerate() {
            let mut bytes = id.to_bytes_be();

            // Ensure we have 32 bytes
            while bytes.len() < blake3::OUT_LEN {
                bytes.insert(0, 0);
            }

            tips_sorted[i] = blake3::Hash::from_bytes(bytes.try_into().unwrap());
        }

        tips_sorted
    }

    /// Perform a topological sort of the DAG.
    pub async fn order_events(&self) -> Vec<blake3::Hash> {
        let mut ordered_events = VecDeque::new();
        let mut visited = HashSet::new();

        for tip in self.get_unreferenced_tips_sorted().await {
            if !visited.contains(&tip) && tip != NULL_ID {
                let tip = self.dag.get(tip.as_bytes()).unwrap().unwrap();
                let tip = deserialize_async(&tip).await.unwrap();
                self.dfs_topological_sort(tip, &mut visited, &mut ordered_events).await;
            }
        }

        ordered_events.make_contiguous().to_vec()
    }

    /// We do a DFS (<https://en.wikipedia.org/wiki/Depth-first_search>), and
    /// additionally we consider the timestamps.
    #[async_recursion]
    async fn dfs_topological_sort(
        &self,
        event: Event,
        visited: &mut HashSet<blake3::Hash>,
        ordered_events: &mut VecDeque<blake3::Hash>,
    ) {
        let event_id = event.id();
        visited.insert(event_id);

        for parent_id in event.parents.iter() {
            if !visited.contains(parent_id) && parent_id != &NULL_ID {
                let p_event = self.dag.get(parent_id.as_bytes()).unwrap().unwrap();
                let p_event = deserialize_async(&p_event).await.unwrap();
                self.dfs_topological_sort(p_event, visited, ordered_events).await;
            }
        }

        // Before inserting, check timestamps to determine the correct position.
        let mut pos = ordered_events.len();
        for (idx, existing_id) in ordered_events.iter().enumerate().rev() {
            assert!(existing_id != &NULL_ID);
            if self.share_same_parents(&event_id, existing_id).await {
                let existing_event = self.dag.get(existing_id.as_bytes()).unwrap().unwrap();
                let existing_event: Event = deserialize_async(&existing_event).await.unwrap();

                // Sort by timestamp
                match event.timestamp.cmp(&existing_event.timestamp) {
                    Ordering::Less => pos = idx,
                    Ordering::Equal => {
                        // In case of a tie-breaker, use the event ID
                        let a = BigUint::from_bytes_be(event_id.as_bytes());
                        let b = BigUint::from_bytes_be(existing_id.as_bytes());
                        if a < b {
                            pos = idx;
                        }
                    }
                    _ => {}
                }
            }
        }

        ordered_events.insert(pos, event_id);
    }

    /// Check if two events have the same parents
    async fn share_same_parents(&self, event_id1: &blake3::Hash, event_id2: &blake3::Hash) -> bool {
        let event1 = self.dag.get(event_id1.as_bytes()).unwrap().unwrap();
        let event1: Event = deserialize_async(&event1).await.unwrap();
        let mut parents1: Vec<_> =
            event1.parents.iter().map(|x| BigUint::from_bytes_be(x.as_bytes())).collect();
        parents1.sort_unstable();

        let event2 = self.dag.get(event_id2.as_bytes()).unwrap().unwrap();
        let event2: Event = deserialize_async(&event2).await.unwrap();
        let mut parents2: Vec<_> =
            event2.parents.iter().map(|x| BigUint::from_bytes_be(x.as_bytes())).collect();
        parents2.sort_unstable();

        parents1 == parents2
    }
}
