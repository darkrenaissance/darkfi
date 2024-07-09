/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use async_recursion::async_recursion;
use darkfi_serial::{deserialize_async, serialize_async};
use log::{debug, error, info, warn};
use num_bigint::BigUint;
use sled_overlay::SledTreeOverlay;
use smol::{
    lock::{OnceCell, RwLock},
    Executor,
};
use tinyjson::JsonValue::{self};

use crate::{
    event_graph::util::{replayer_log, seconds_until_next_rotation},
    net::P2pPtr,
    rpc::{
        jsonrpc::{JsonResponse, JsonResult},
        util::json_map,
    },
    system::{
        sleep, timeout::timeout, Publisher, PublisherPtr, StoppableTask, StoppableTaskPtr,
        Subscription,
    },
    Error, Result,
};

/// An event graph event
pub mod event;
pub use event::Event;

/// P2P protocol implementation for the Event Graph
pub mod proto;
use proto::{EventRep, EventReq, TipRep, TipReq};

/// Utility functions
pub mod util;
use util::{generate_genesis, next_rotation_timestamp};

// Debugging event graph
pub(crate) mod deg;
use deg::DegEvent;

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

    datastore: PathBuf,

    replay_mode: bool,
    /// The set of unreferenced DAG tips
    unreferenced_tips: RwLock<BTreeMap<u64, HashSet<blake3::Hash>>>,
    /// A `HashSet` containg event IDs and their 1-level parents.
    /// These come from the events we've sent out using `EventPut`.
    /// They are used with `EventReq` to decide if we should reply
    /// or not. Additionally it is also used when we broadcast the
    /// `TipRep` message telling peers about our unreferenced tips.
    broadcasted_ids: RwLock<HashSet<blake3::Hash>>,
    /// DAG Pruning Task
    prune_task: OnceCell<StoppableTaskPtr>,
    /// Event publisher, this notifies whenever an event is
    /// inserted into the DAG
    pub event_pub: PublisherPtr<Event>,
    /// Current genesis event
    current_genesis: RwLock<Event>,
    /// Currently configured DAG rotation, in days
    days_rotation: u64,
    /// Flag signalling DAG has finished initial sync
    pub synced: RwLock<bool>,
    /// Enable graph debugging
    pub deg_enabled: RwLock<bool>,
    /// The publisher for which we can give deg info over
    deg_publisher: PublisherPtr<DegEvent>,
}

impl EventGraph {
    /// Create a new [`EventGraph`] instance.
    /// * `days_rotation` marks the lifetime of the DAG before it's pruned.
    pub async fn new(
        p2p: P2pPtr,
        sled_db: sled::Db,
        datastore: PathBuf,
        replay_mode: bool,
        dag_tree_name: &str,
        days_rotation: u64,
        ex: Arc<Executor<'_>>,
    ) -> Result<EventGraphPtr> {
        let dag = sled_db.open_tree(dag_tree_name)?;
        let unreferenced_tips = RwLock::new(BTreeMap::new());
        let broadcasted_ids = RwLock::new(HashSet::new());
        let event_pub = Publisher::new();

        // Create the current genesis event based on the `days_rotation`
        let current_genesis = generate_genesis(days_rotation);
        let self_ = Arc::new(Self {
            p2p,
            dag: dag.clone(),
            datastore,
            replay_mode,
            unreferenced_tips,
            broadcasted_ids,
            prune_task: OnceCell::new(),
            event_pub,
            current_genesis: RwLock::new(current_genesis.clone()),
            days_rotation,
            synced: RwLock::new(false),
            deg_enabled: RwLock::new(false),
            deg_publisher: Publisher::new(),
        });

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

    pub fn days_rotation(&self) -> u64 {
        self.days_rotation
    }

    async fn _handle_stop(&self, sled_db: sled::Db) {
        info!(target: "event_graph::_handle_stop()", "[EVENTGRAPH] Prune task stopped, flushing sled");
        sled_db.flush_async().await.unwrap();
    }

    /// Sync the DAG from connected peers
    pub async fn dag_sync(&self) -> Result<()> {
        // We do an optimistic sync where we ask all our connected peers for
        // the latest layer DAG tips (unreferenced events) and then we accept
        // the ones we see the most times.
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
        let channels = self.p2p.hosts().channels();
        let mut communicated_peers = channels.len();
        info!(
            target: "event_graph::dag_sync()",
            "[EVENTGRAPH] Syncing DAG from {} peers...", communicated_peers,
        );

        // Here we keep track of the tips, their layers and how many time we've seen them.
        let mut tips: HashMap<blake3::Hash, (u64, usize)> = HashMap::new();

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

            let peer_tips = match timeout(
                Duration::from_secs(self.p2p.settings().read().await.outbound_connect_timeout),
                tip_rep_sub.receive(),
            )
            .await
            {
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
            for (layer, layer_tips) in peer_tips {
                for tip in layer_tips {
                    if let Some(seen_tip) = tips.get_mut(tip) {
                        seen_tip.1 += 1;
                    } else {
                        tips.insert(*tip, (*layer, 1));
                    }
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

        // We know the number of peers we've communicated with,
        // so we will consider events we saw at more than 2/3 of
        // those peers.
        let consideration_threshold = communicated_peers * 2 / 3;
        let mut considered_tips = HashSet::new();
        for (tip, (_, amount)) in tips.iter() {
            if amount > &consideration_threshold {
                considered_tips.insert(*tip);
            }
        }
        drop(tips);

        // Now begin fetching the events backwards.
        let mut missing_parents = HashSet::new();
        for tip in considered_tips.iter() {
            assert!(tip != &NULL_ID);

            if !self.dag.contains_key(tip.as_bytes()).unwrap() {
                missing_parents.insert(*tip);
            }
        }

        if missing_parents.is_empty() {
            *self.synced.write().await = true;
            info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] DAG synced successfully!");
            return Ok(())
        }

        info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] Fetching events");
        let mut received_events: BTreeMap<u64, Vec<Event>> = BTreeMap::new();
        let mut received_events_hashes = HashSet::new();

        while !missing_parents.is_empty() {
            let mut found_event = false;

            for channel in channels.iter() {
                let url = channel.address();

                debug!(
                    target: "event_graph::dag_sync()",
                    "Requesting {:?} from {}...", missing_parents, url,
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

                let request_missing_events = missing_parents.clone().into_iter().collect();
                if let Err(e) = channel.send(&EventReq(request_missing_events)).await {
                    error!(
                        target: "event_graph::dag_sync()",
                        "[EVENTGRAPH] Sync: Failed communicating EventReq({:?}) to {}: {}",
                        missing_parents, url, e,
                    );
                    continue
                }

                let parent = match timeout(
                    Duration::from_secs(self.p2p.settings().read().await.outbound_connect_timeout),
                    ev_rep_sub.receive(),
                )
                .await
                {
                    Ok(parent) => parent,
                    Err(_) => {
                        error!(
                            target: "event_graph::dag_sync()",
                            "[EVENTGRAPH] Sync: Timeout waiting for parents {:?} from {}",
                            missing_parents, url,
                        );
                        continue
                    }
                };

                let parents = match parent {
                    Ok(v) => v.0.clone(),
                    Err(e) => {
                        error!(
                            target: "event_graph::dag_sync()",
                            "[EVENTGRAPH] Sync: Failed receiving parents {:?}: {}",
                            missing_parents, e,
                        );
                        continue
                    }
                };

                for parent in parents {
                    let parent_id = parent.id();
                    if !missing_parents.contains(&parent_id) {
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

                    if let Some(layer_events) = received_events.get_mut(&parent.layer) {
                        layer_events.push(parent.clone());
                    } else {
                        let layer_events = vec![parent.clone()];
                        received_events.insert(parent.layer, layer_events);
                    }
                    received_events_hashes.insert(parent_id);

                    missing_parents.remove(&parent_id);
                    found_event = true;

                    // See if we have the upper parents
                    for upper_parent in parent.parents.iter() {
                        if upper_parent == &NULL_ID {
                            continue
                        }

                        if !missing_parents.contains(upper_parent) &&
                            !received_events_hashes.contains(upper_parent) &&
                            !self.dag.contains_key(upper_parent.as_bytes()).unwrap()
                        {
                            debug!(
                                target: "event_graph::dag_sync()",
                                "Found upper missing parent event{}", upper_parent,
                            );
                            missing_parents.insert(*upper_parent);
                        }
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
        } // <-- while !missing_parents.is_empty

        // At this point we should've got all the events.
        // We should add them to the DAG.
        let mut events = vec![];
        for (_, tips) in received_events {
            for tip in tips {
                events.push(tip);
            }
        }
        self.dag_insert(&events).await?;

        *self.synced.write().await = true;

        info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] DAG synced successfully!");
        Ok(())
    }

    /// Atomically prune the DAG and insert the given event as genesis.
    async fn dag_prune(&self, genesis_event: Event) -> Result<()> {
        debug!(target: "event_graph::dag_prune()", "Pruning DAG...");

        // Acquire exclusive locks to unreferenced_tips, broadcasted_ids and
        // current_genesis while this operation is happening. We do this to
        // ensure that during the pruning operation, no other operations are
        // able to access the intermediate state which could lead to producing
        // the wrong state after pruning.
        let mut unreferenced_tips = self.unreferenced_tips.write().await;
        let mut broadcasted_ids = self.broadcasted_ids.write().await;
        let mut current_genesis = self.current_genesis.write().await;

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
        *unreferenced_tips = BTreeMap::new();
        unreferenced_tips.insert(0, HashSet::from([genesis_event.id()]));
        *current_genesis = genesis_event;
        *broadcasted_ids = HashSet::new();
        drop(unreferenced_tips);
        drop(broadcasted_ids);
        drop(current_genesis);

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
                layer: 0,
            };

            // Sleep until it's time to rotate.
            let s = seconds_until_next_rotation(next_rotation);

            debug!(target: "event_graph::dag_prune_task()", "Sleeping {}s until next DAG prune", s);
            sleep(s).await;
            debug!(target: "event_graph::dag_prune_task()", "Rotation period reached");

            // Trigger DAG prune
            self.dag_prune(current_genesis).await?;
        }
    }

    /// Atomically insert given events into the DAG and return the event IDs.
    /// All provided events must be valid. An overlay is used over the DAG tree,
    /// temporary writting each event in order. After all events have been
    /// validated and inserted successfully, we write the overlay to sled.
    /// This will append the new events into the unreferenced tips set, and
    /// remove the events' parents from it. It will also append the events'
    /// level-1 parents to the `broadcasted_ids` set, so the P2P protocol
    /// knows that any requests for them are actually legitimate.
    /// TODO: The `broadcasted_ids` set should periodically be pruned, when
    /// some sensible time has passed after broadcasting the event.
    pub async fn dag_insert(&self, events: &[Event]) -> Result<Vec<blake3::Hash>> {
        // Sanity check
        if events.is_empty() {
            return Ok(vec![])
        }

        // Acquire exclusive locks to `unreferenced_tips and broadcasted_ids`
        let mut unreferenced_tips = self.unreferenced_tips.write().await;
        let mut broadcasted_ids = self.broadcasted_ids.write().await;

        // Here we keep the IDs to return
        let mut ids = Vec::with_capacity(events.len());

        // Create an overlay over the DAG tree
        let mut overlay = SledTreeOverlay::new(&self.dag);

        // Grab genesis timestamp
        let genesis_timestamp = self.current_genesis.read().await.timestamp;

        // Iterate over given events to validate them and
        // write them to the overlay
        for event in events {
            let event_id = event.id();
            debug!(
                target: "event_graph::dag_insert()",
                "Inserting event {} into the DAG", event_id,
            );

            if !event
                .validate(&self.dag, genesis_timestamp, self.days_rotation, Some(&overlay))
                .await?
            {
                error!(target: "event_graph::dag_insert()", "Event {} is invalid!", event_id);
                return Err(Error::EventIsInvalid)
            }

            let event_se = serialize_async(event).await;

            // Add the event to the overlay
            overlay.insert(event_id.as_bytes(), &event_se)?;

            if self.replay_mode {
                replayer_log(&self.datastore, "insert".to_owned(), event_se)?;
            }
            // Note down the event ID to return
            ids.push(event_id);
        }

        // Aggregate changes into a single batch
        let batch = overlay.aggregate().unwrap();

        // Atomically apply the batch.
        // Panic if something is corrupted.
        if let Err(e) = self.dag.apply_batch(batch) {
            panic!("Failed applying dag_insert batch to sled: {}", e);
        }

        // Iterate over given events to update references and
        // send out notifications about them
        for event in events {
            let event_id = event.id();

            // Update the unreferenced DAG tips set
            debug!(
                target: "event_graph::dag_insert()",
                "Event {} parents {:#?}", event_id, event.parents,
            );
            for parent_id in event.parents.iter() {
                if parent_id != &NULL_ID {
                    debug!(
                        target: "event_graph::dag_insert()",
                        "Removing {} from unreferenced_tips", parent_id,
                    );

                    // Iterate over unreferenced tips in previous layers
                    // and remove the parent
                    // NOTE: this might be too exhaustive, but the
                    // assumption is that previous layers unreferenced
                    // tips will be few.
                    for (layer, tips) in unreferenced_tips.iter_mut() {
                        if layer >= &event.layer {
                            continue
                        }
                        tips.remove(parent_id);
                    }
                    broadcasted_ids.insert(*parent_id);
                }
            }
            unreferenced_tips.retain(|_, tips| !tips.is_empty());
            debug!(
                target: "event_graph::dag_insert()",
                "Adding {} to unreferenced tips", event_id,
            );

            if let Some(layer_tips) = unreferenced_tips.get_mut(&event.layer) {
                layer_tips.insert(event_id);
            } else {
                let mut layer_tips = HashSet::new();
                layer_tips.insert(event_id);
                unreferenced_tips.insert(event.layer, layer_tips);
            }

            // Send out notifications about the new event
            self.event_pub.notify(event.clone()).await;
        }

        // Drop the exclusive locks
        drop(unreferenced_tips);
        drop(broadcasted_ids);

        Ok(ids)
    }

    /// Fetch an event from the DAG
    pub async fn dag_get(&self, event_id: &blake3::Hash) -> Result<Option<Event>> {
        let Some(bytes) = self.dag.get(event_id.as_bytes())? else { return Ok(None) };
        let event: Event = deserialize_async(&bytes).await?;

        Ok(Some(event))
    }

    /// Get next layer along with its N_EVENT_PARENTS from the unreferenced
    /// tips of the DAG. Since tips are mapped by their layer, we go backwards
    /// until we fill the vector, ensuring we always use latest layers tips as
    /// parents.
    async fn get_next_layer_with_parents(&self) -> (u64, [blake3::Hash; N_EVENT_PARENTS]) {
        let unreferenced_tips = self.unreferenced_tips.read().await;

        let mut parents = [NULL_ID; N_EVENT_PARENTS];
        let mut index = 0;
        'outer: for (_, tips) in unreferenced_tips.iter().rev() {
            for tip in tips.iter() {
                parents[index] = *tip;
                index += 1;
                if index >= N_EVENT_PARENTS {
                    break 'outer
                }
            }
        }

        let next_layer = unreferenced_tips.last_key_value().unwrap().0 + 1;

        assert!(parents.iter().any(|x| x != &NULL_ID));
        (next_layer, parents)
    }

    /// Find the unreferenced tips in the current DAG state, mapped by their layers.
    async fn find_unreferenced_tips(&self) -> BTreeMap<u64, HashSet<blake3::Hash>> {
        // First get all the event IDs
        let mut tips = HashSet::new();
        for iter_elem in self.dag.iter() {
            let (id, _) = iter_elem.unwrap();
            let id = blake3::Hash::from_bytes((&id as &[u8]).try_into().unwrap());
            tips.insert(id);
        }

        // Iterate again to find unreferenced IDs
        for iter_elem in self.dag.iter() {
            let (_, event) = iter_elem.unwrap();
            let event: Event = deserialize_async(&event).await.unwrap();
            for parent in event.parents.iter() {
                tips.remove(parent);
            }
        }

        // Build the layers map
        let mut map: BTreeMap<u64, HashSet<blake3::Hash>> = BTreeMap::new();
        for tip in tips {
            let event = self.dag_get(&tip).await.unwrap().unwrap();
            if let Some(layer_tips) = map.get_mut(&event.layer) {
                layer_tips.insert(tip);
            } else {
                let mut layer_tips = HashSet::new();
                layer_tips.insert(tip);
                map.insert(event.layer, layer_tips);
            }
        }

        map
    }

    /// Internal function used for DAG sorting.
    async fn get_unreferenced_tips_sorted(&self) -> [blake3::Hash; N_EVENT_PARENTS] {
        let (_, tips) = self.get_next_layer_with_parents().await;

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
                let tip = self.dag_get(&tip).await.unwrap().unwrap();
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
                let p_event = self.dag_get(parent_id).await.unwrap().unwrap();
                self.dfs_topological_sort(p_event, visited, ordered_events).await;
            }
        }

        // Before inserting, check timestamps to determine the correct position.
        let mut pos = ordered_events.len();
        for (idx, existing_id) in ordered_events.iter().enumerate().rev() {
            assert!(existing_id != &NULL_ID);
            if self.share_same_parents(&event_id, existing_id).await {
                let existing_event = self.dag_get(existing_id).await.unwrap().unwrap();

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
        let event1 = self.dag_get(event_id1).await.unwrap().unwrap();
        let mut parents1: Vec<_> =
            event1.parents.iter().map(|x| BigUint::from_bytes_be(x.as_bytes())).collect();
        parents1.sort_unstable();

        let event2 = self.dag_get(event_id2).await.unwrap().unwrap();
        let mut parents2: Vec<_> =
            event2.parents.iter().map(|x| BigUint::from_bytes_be(x.as_bytes())).collect();
        parents2.sort_unstable();

        parents1 == parents2
    }

    /// Enable graph debugging
    pub async fn deg_enable(&self) {
        *self.deg_enabled.write().await = true;
        warn!("[EVENTGRAPH] Graph debugging enabled!");
    }

    /// Disable graph debugging
    pub async fn deg_disable(&self) {
        *self.deg_enabled.write().await = false;
        warn!("[EVENTGRAPH] Graph debugging disabled!");
    }

    /// Subscribe to deg events
    pub async fn deg_subscribe(&self) -> Subscription<DegEvent> {
        self.deg_publisher.clone().subscribe().await
    }

    /// Send a deg notification over the publisher
    pub async fn deg_notify(&self, event: DegEvent) {
        self.deg_publisher.notify(event).await;
    }

    pub async fn eventgraph_info(&self, id: u16, _params: JsonValue) -> JsonResult {
        let mut graph = HashMap::new();
        for iter_elem in self.dag.iter() {
            let (id, val) = iter_elem.unwrap();
            let id = blake3::Hash::from_bytes((&id as &[u8]).try_into().unwrap());
            let val: Event = deserialize_async(&val).await.unwrap();
            graph.insert(id, val);
        }

        let json_graph = graph
            .into_iter()
            .map(|(k, v)| {
                let key = k.to_string();
                let value = JsonValue::from(v);
                (key, value)
            })
            .collect();
        let values = json_map([("dag", JsonValue::Object(json_graph))]);

        let result = JsonValue::Object(HashMap::from([("eventgraph_info".to_string(), values)]));

        JsonResponse::new(result, id).into()
    }
}
