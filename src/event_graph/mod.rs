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
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};

use blake3::Hash;
use darkfi_serial::{deserialize_async, serialize_async};
use log::{debug, error, info, warn};
use num_bigint::BigUint;
use sled_overlay::{sled, SledTreeOverlay};
use smol::{
    lock::{OnceCell, RwLock},
    Executor,
};
use tinyjson::JsonValue::{self};

use crate::{
    event_graph::util::replayer_log,
    net::P2pPtr,
    rpc::{
        jsonrpc::{JsonResponse, JsonResult},
        util::json_map,
    },
    system::{msleep, Publisher, PublisherPtr, StoppableTask, StoppableTaskPtr, Subscription},
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
use util::{
    generate_genesis, midnight_timestamp, millis_until_next_rotation, next_rotation_timestamp,
};

// Debugging event graph
pub mod deg;
use deg::DegEvent;

#[cfg(test)]
mod tests;

/// Initial genesis timestamp in millis (07 Sep 2023, 00:00:00 UTC)
/// Must always be UTC midnight.
pub const INITIAL_GENESIS: u64 = 1_694_044_800_000;
/// Genesis event contents
pub const GENESIS_CONTENTS: &[u8] = &[0x47, 0x45, 0x4e, 0x45, 0x53, 0x49, 0x53];

/// The number of parents an event is supposed to have.
pub const N_EVENT_PARENTS: usize = 5;
/// Allowed timestamp drift in milliseconds
const EVENT_TIME_DRIFT: u64 = 60_000;
/// Null event ID
pub const NULL_ID: blake3::Hash = blake3::Hash::from_bytes([0x00; blake3::OUT_LEN]);

/// Maximum number of DAGs to store, this should be configurable
pub const DAGS_MAX_NUMBER: i8 = 5;

/// Atomic pointer to an [`EventGraph`] instance.
pub type EventGraphPtr = Arc<EventGraph>;
pub type LayerUTips = BTreeMap<u64, HashSet<blake3::Hash>>;

#[derive(Clone)]
pub struct DAGStore {
    db: sled::Db,
    dags: HashMap<Hash, (sled::Tree, LayerUTips)>,
}

impl DAGStore {
    pub async fn new(&self, sled_db: sled::Db, days_rotation: u64) -> Self {
        let mut considered_trees = HashMap::new();
        if days_rotation > 0 {
            // Create previous genesises if not existing, since they are deterministic.
            for i in 1..=DAGS_MAX_NUMBER {
                let i_days_ago = midnight_timestamp((i - DAGS_MAX_NUMBER).into());
                let genesis = Event {
                    timestamp: i_days_ago,
                    content: GENESIS_CONTENTS.to_vec(),
                    parents: [NULL_ID; N_EVENT_PARENTS],
                    layer: 0,
                };

                let tree_name = genesis.id().to_string();
                let dag = sled_db.open_tree(&tree_name).unwrap();
                if dag.is_empty() {
                    let mut overlay = SledTreeOverlay::new(&dag);

                    let event_se = serialize_async(&genesis).await;

                    // Add the event to the overlay
                    overlay.insert(genesis.id().as_bytes(), &event_se).unwrap();

                    // Aggregate changes into a single batch
                    let batch = overlay.aggregate().unwrap();

                    // Atomically apply the batch.
                    // Panic if something is corrupted.
                    if let Err(e) = dag.apply_batch(batch) {
                        panic!("Failed applying dag_insert batch to sled: {}", e);
                    }
                }
                let utips = self.find_unreferenced_tips(&dag).await;
                considered_trees.insert(genesis.id(), (dag, utips));
            }
        } else {
            let genesis = generate_genesis(0);
            let tree_name = genesis.id().to_string();
            let dag = sled_db.open_tree(&tree_name).unwrap();
            if dag.is_empty() {
                let mut overlay = SledTreeOverlay::new(&dag);

                let event_se = serialize_async(&genesis).await;

                // Add the event to the overlay
                overlay.insert(genesis.id().as_bytes(), &event_se).unwrap();

                // Aggregate changes into a single batch
                let batch = overlay.aggregate().unwrap();

                // Atomically apply the batch.
                // Panic if something is corrupted.
                if let Err(e) = dag.apply_batch(batch) {
                    panic!("Failed applying dag_insert batch to sled: {}", e);
                }
            }
            let utips = self.find_unreferenced_tips(&dag).await;
            considered_trees.insert(genesis.id(), (dag, utips));
        }

        Self { db: sled_db, dags: considered_trees }
    }

    /// Adds a DAG into the set of DAGs and drops the oldest one if exeeding DAGS_MAX_NUMBER,
    /// This is called if prune_task activates.
    pub async fn add_dag(&mut self, dag_name: &str, genesis_event: &Event) {
        debug!("add_dag::dags: {}", self.dags.len());
        if self.dags.len() > DAGS_MAX_NUMBER.try_into().unwrap() {
            while self.dags.len() >= DAGS_MAX_NUMBER.try_into().unwrap() {
                debug!("[EVENTGRAPH] dropping oldest dag");
                let sorted_dags = self.sort_dags().await;
                // since dags are sorted in reverse
                let oldest_tree = sorted_dags.last().unwrap().name();
                let oldest_key = String::from_utf8_lossy(&oldest_tree);

                let oldest_key = blake3::Hash::from_str(&oldest_key).unwrap();
                let oldest_tree = self.dags.remove(&oldest_key).unwrap();
                self.db.drop_tree(oldest_tree.0.name()).unwrap();
            }
        }

        // Insert genesis
        let dag = self.db.open_tree(dag_name).unwrap();
        dag.insert(genesis_event.id().as_bytes(), serialize_async(genesis_event).await).unwrap();
        let utips = self.find_unreferenced_tips(&dag).await;
        self.dags.insert(genesis_event.id(), (dag, utips));
    }

    // Get a DAG provifing its name.
    pub fn get_dag(&self, dag_name: &str) -> sled::Tree {
        self.db.open_tree(dag_name).unwrap()
    }

    /// Get {count} many DAGs.
    pub async fn get_dags(&self, count: usize) -> Vec<sled::Tree> {
        let sorted_dags = self.sort_dags().await;
        sorted_dags.into_iter().take(count).collect()
    }

    /// Sort DAGs chronologically
    async fn sort_dags(&self) -> Vec<sled::Tree> {
        let mut vec_dags = vec![];

        let dags = self
            .dags
            .iter()
            .map(|x| {
                let trees = x.1;
                trees.0.clone()
            })
            .collect::<Vec<_>>();

        for dag in dags {
            let genesis = dag.first().unwrap().unwrap().1;
            let genesis_event: Event = deserialize_async(&genesis).await.unwrap();
            vec_dags.push((genesis_event.timestamp, dag));
        }

        vec_dags.sort_by_key(|&(ts, _)| ts);
        vec_dags.reverse();

        vec_dags.into_iter().map(|(_, dag)| dag).collect()
    }

    /// Find the unreferenced tips in the current DAG state, mapped by their layers.
    async fn find_unreferenced_tips(&self, dag: &sled::Tree) -> LayerUTips {
        // First get all the event IDs
        let mut tips = HashSet::new();
        for iter_elem in dag.iter() {
            let (id, _) = iter_elem.unwrap();
            let id = blake3::Hash::from_bytes((&id as &[u8]).try_into().unwrap());
            tips.insert(id);
        }
        // Iterate again to find unreferenced IDs
        for iter_elem in dag.iter() {
            let (_, event) = iter_elem.unwrap();
            let event: Event = deserialize_async(&event).await.unwrap();
            for parent in event.parents.iter() {
                tips.remove(parent);
            }
        }
        // Build the layers map
        let mut map: LayerUTips = BTreeMap::new();
        for tip in tips {
            let event = self.fetch_event(&tip, &dag).await.unwrap().unwrap();
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

    /// Fetch an event from the DAG
    pub async fn fetch_event(
        &self,
        event_id: &blake3::Hash,
        dag: &sled::Tree,
    ) -> Result<Option<Event>> {
        let Some(bytes) = dag.get(event_id.as_bytes())? else {
            return Ok(None);
        };
        let event: Event = deserialize_async(&bytes).await?;

        return Ok(Some(event))
    }
}

/// An Event Graph instance
pub struct EventGraph {
    /// Pointer to the P2P network instance
    p2p: P2pPtr,
    /// Sled tree containing the DAG
    dag_store: RwLock<DAGStore>,
    /// Replay logs path.
    datastore: PathBuf,
    /// Run in replay_mode where if set we log Sled DB instructions
    /// into `datastore`, useful to reacreate a faulty DAG to debug.
    replay_mode: bool,
    /// A `HashSet` containg event IDs and their 1-level parents.
    /// These come from the events we've sent out using `EventPut`.
    /// They are used with `EventReq` to decide if we should reply
    /// or not. Additionally it is also used when we broadcast the
    /// `TipRep` message telling peers about our unreferenced tips.
    broadcasted_ids: RwLock<HashSet<blake3::Hash>>,
    /// DAG Pruning Task
    pub prune_task: OnceCell<StoppableTaskPtr>,
    /// Event publisher, this notifies whenever an event is
    /// inserted into the DAG
    pub event_pub: PublisherPtr<Event>,
    /// Current genesis event
    pub current_genesis: RwLock<Event>,
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
    /// Create a new [`EventGraph`] instance, creates a new Genesis
    /// event and checks if it is containd in DAG, if not prunes DAG,
    /// may also start a pruning task based on `days_rotation`, and
    /// return an atomic instance of
    /// `Self`
    /// * `p2p` atomic pointer to p2p.
    /// * `sled_db` sled DB instance.
    /// * `datastore` path where we should log db instrucion if run in
    ///   replay mode.
    /// * `replay_mode` set the flag to keep a log of db instructions.
    /// * `days_rotation` marks the lifetime of the DAG before it's
    ///   pruned.
    pub async fn new(
        p2p: P2pPtr,
        sled_db: sled::Db,
        datastore: PathBuf,
        replay_mode: bool,
        days_rotation: u64,
        ex: Arc<Executor<'_>>,
    ) -> Result<EventGraphPtr> {
        let broadcasted_ids = RwLock::new(HashSet::new());
        let event_pub = Publisher::new();

        // Create the current genesis event based on the `days_rotation`
        let current_genesis = generate_genesis(days_rotation);

        let current_dag_tree_name = current_genesis.id().to_string();
        let dag_store = DAGStore { db: sled_db.clone(), dags: HashMap::default() }
            .new(sled_db, days_rotation)
            .await;

        let self_ = Arc::new(Self {
            p2p,
            dag_store: RwLock::new(dag_store.clone()),
            datastore,
            replay_mode,
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
        let dag = dag_store.get_dag(&current_dag_tree_name);
        if !dag.contains_key(current_genesis.id().as_bytes())? {
            info!(
                target: "event_graph::new()",
                "[EVENTGRAPH] DAG does not contain current genesis, pruning existing data",
            );
            self_.dag_prune(current_genesis).await?;
        }

        // Spawn the DAG pruning task
        if days_rotation > 0 {
            let prune_task = StoppableTask::new();
            let _ = self_.prune_task.set(prune_task.clone()).await;

            prune_task.clone().start(
                self_.clone().dag_prune_task(days_rotation),
                |res| async move {
                    match res {
                        Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                        Err(e) => error!(target: "event_graph::_handle_stop()", "[EVENTGRAPH] Failed stopping prune task: {e}")
                    }
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

    /// Sync the DAG from connected peers
    pub async fn dag_sync(&self, dag: sled::Tree) -> Result<()> {
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

        let dag_name = String::from_utf8_lossy(&dag.name()).to_string();

        // Get references to all our peers.
        let channels = self.p2p.hosts().peers();
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

            if let Err(e) = channel.send(&TipReq(dag_name.clone())).await {
                error!(
                    target: "event_graph::dag_sync()",
                    "[EVENTGRAPH] Sync: Couldn't contact peer {}, skipping ({})", url, e,
                );
                communicated_peers -= 1;
                continue
            };

            // Node waits for response
            let Ok(peer_tips) = tip_rep_sub
                .receive_with_timeout(self.p2p.settings().read().await.outbound_connect_timeout)
                .await
            else {
                error!(
                    target: "event_graph::dag_sync()",
                    "[EVENTGRAPH] Sync: Peer {} didn't reply with tips in time, skipping", url,
                );
                communicated_peers -= 1;
                continue
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

            if !dag.contains_key(tip.as_bytes()).unwrap() {
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

                // Node waits for response
                let Ok(parent) = ev_rep_sub
                    .receive_with_timeout(self.p2p.settings().read().await.outbound_connect_timeout)
                    .await
                else {
                    error!(
                        target: "event_graph::dag_sync()",
                        "[EVENTGRAPH] Sync: Timeout waiting for parents {:?} from {}",
                        missing_parents, url,
                    );
                    continue
                };

                let parents = parent.0.clone();

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
                            !dag.contains_key(upper_parent.as_bytes()).unwrap()
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
        self.dag_insert(&events, &dag_name).await?;

        *self.synced.write().await = true;

        info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] DAG synced successfully!");
        Ok(())
    }

    /// Choose how many dags to sync
    pub async fn sync_selected(&self, count: usize) -> Result<()> {
        let mut dags_to_sync = self.dag_store.read().await.get_dags(count).await;
        // Since get_dags() return sorted dags in reverse
        dags_to_sync.reverse();
        for dag in dags_to_sync {
            match self.dag_sync(dag).await {
                Ok(()) => continue,
                Err(e) => {
                    return Err(e);
                }
            }
        }

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
        let mut broadcasted_ids = self.broadcasted_ids.write().await;
        let mut current_genesis = self.current_genesis.write().await;

        // Add the DAG to DAGStore and start a new one
        let dag_name = genesis_event.id().to_string();
        self.dag_store.write().await.add_dag(&dag_name, &genesis_event).await;

        // Clear unreferenced tips and bcast ids
        *current_genesis = genesis_event;
        *broadcasted_ids = HashSet::new();
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
            let s = millis_until_next_rotation(next_rotation);

            debug!(target: "event_graph::dag_prune_task()", "Sleeping {}ms until next DAG prune", s);
            msleep(s).await;
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
    pub async fn dag_insert(&self, events: &[Event], dag_name: &str) -> Result<Vec<blake3::Hash>> {
        // Sanity check
        if events.is_empty() {
            return Ok(vec![])
        }

        // Acquire exclusive locks to `unreferenced_tips and broadcasted_ids`
        // let mut unreferenced_tips = self.unreferenced_tips.write().await;
        let dag_name_hash = blake3::Hash::from_str(dag_name).unwrap();

        let mut broadcasted_ids = self.broadcasted_ids.write().await;

        let dag = self.dag_store.read().await.get_dag(dag_name);

        // Here we keep the IDs to return
        let mut ids = Vec::with_capacity(events.len());

        // Create an overlay over the DAG tree
        let mut overlay = SledTreeOverlay::new(&dag);

        // Iterate over given events to validate them and
        // write them to the overlay
        for event in events {
            let event_id = event.id();
            debug!(
                target: "event_graph::dag_insert()",
                "Inserting event {} into the DAG", event_id,
            );

            if !event.validate(&dag, self.days_rotation, Some(&overlay)).await? {
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
        if let Err(e) = dag.apply_batch(batch) {
            panic!("Failed applying dag_insert batch to sled: {}", e);
        }

        drop(dag);
        // let (_, mut unreferenced_tips) =
        //     self.dag_store.write().await.dags.get_mut(&dag_name_hash).unwrap();
        let mut dag_store = self.dag_store.write().await;
        let (_, unreferenced_tips) = &mut dag_store.dags.get_mut(&dag_name_hash).unwrap();

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
        drop(dag_store);
        drop(broadcasted_ids);

        Ok(ids)
    }

    /// Search and fetch an event through all DAGs
    pub async fn fetch_event(&self, event_id: &blake3::Hash) -> Result<Option<Event>> {
        let store = self.dag_store.read().await;
        for tree_elem in store.dags.clone() {
            let dag_name = tree_elem.0.to_string();
            let Some(bytes) = store.get_dag(&dag_name).get(event_id.as_bytes())? else {
                continue;
            };
            let event: Event = deserialize_async(&bytes).await?;

            return Ok(Some(event))
        }

        Ok(None)
    }

    /// Get next layer along with its N_EVENT_PARENTS from the unreferenced
    /// tips of the DAG. Since tips are mapped by their layer, we go backwards
    /// until we fill the vector, ensuring we always use latest layers tips as
    /// parents.
    async fn get_next_layer_with_parents(
        &self,
        dag_name: &Hash,
    ) -> (u64, [blake3::Hash; N_EVENT_PARENTS]) {
        let store = self.dag_store.read().await;
        let (_, unreferenced_tips) = store.dags.get(dag_name).unwrap();

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

    /// Internal function used for DAG sorting.
    async fn get_unreferenced_tips_sorted(&self) -> Vec<[blake3::Hash; N_EVENT_PARENTS]> {
        let mut vec_tips = vec![];
        let mut tips_sorted = [NULL_ID; N_EVENT_PARENTS];
        for (_, (dag, _)) in self.dag_store.read().await.dags.iter() {
            let dags_names = String::from_utf8_lossy(&dag.name()).to_string();
            let dags_name_hashes = blake3::Hash::from_str(&dags_names).unwrap();
            let (_, tips) = self.get_next_layer_with_parents(&dags_name_hashes).await;
            // Convert the hash to BigUint for sorting
            let mut sorted: Vec<_> =
                tips.iter().map(|x| BigUint::from_bytes_be(x.as_bytes())).collect();
            sorted.sort_unstable();

            // Convert back to blake3
            for (i, id) in sorted.iter().enumerate() {
                let mut bytes = id.to_bytes_be();

                // Ensure we have 32 bytes
                while bytes.len() < blake3::OUT_LEN {
                    bytes.insert(0, 0);
                }

                tips_sorted[i] = blake3::Hash::from_bytes(bytes.try_into().unwrap());
            }

            vec_tips.push(tips_sorted);
        }

        vec_tips
    }

    /// Perform a topological sort of the DAG.
    pub async fn order_events(&self) -> Vec<Event> {
        let mut ordered_events = VecDeque::new();
        let mut visited = HashSet::new();

        for i in self.get_unreferenced_tips_sorted().await {
            for tip in i {
                if !visited.contains(&tip) && tip != NULL_ID {
                    let tip = self.fetch_event(&tip).await.unwrap().unwrap();
                    ordered_events.extend(self.dfs_topological_sort(tip, &mut visited).await);
                }
            }
        }

        let mut ord_events_vec = ordered_events.make_contiguous().to_vec();
        // Order events by timestamp.
        ord_events_vec.sort_unstable_by(|a, b| a.1.timestamp.cmp(&b.1.timestamp));
        // ord_events_vec.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.timestamp.cmp(&b.1.timestamp)));

        ord_events_vec.iter().map(|a| a.1.clone()).collect::<Vec<Event>>()
    }

    /// We do a non-recursive DFS (<https://en.wikipedia.org/wiki/Depth-first_search>),
    /// and additionally we consider the timestamps.
    async fn dfs_topological_sort(
        &self,
        event: Event,
        visited: &mut HashSet<blake3::Hash>,
    ) -> VecDeque<(u64, Event)> {
        let mut ordered_events = VecDeque::new();
        let mut stack = VecDeque::new();
        let event_id = event.id();
        stack.push_back(event_id);

        while let Some(event_id) = stack.pop_front() {
            if !visited.contains(&event_id) && event_id != NULL_ID {
                visited.insert(event_id);
                if let Some(event) = self.fetch_event(&event_id).await.unwrap() {
                    for parent in event.parents.iter() {
                        stack.push_back(*parent);
                    }

                    ordered_events.push_back((event.layer, event))
                }
            }
        }

        ordered_events
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
        let current_genesis = self.current_genesis.read().await;
        let dag_name = current_genesis.id().to_string();
        let mut graph = HashMap::new();
        for iter_elem in self.dag_store.read().await.get_dag(&dag_name).iter() {
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

    /// Fetch all the events that are on a higher layers than the
    /// provided ones.
    pub async fn fetch_successors_of(&self, tips: LayerUTips) -> Result<Vec<Event>> {
        debug!(
             target: "event_graph::fetch_successors_of()",
             "fetching successors of {tips:?}"
        );

        let current_genesis = self.current_genesis.read().await;
        let dag_name = current_genesis.id().to_string();
        let mut graph = HashMap::new();
        for iter_elem in self.dag_store.read().await.get_dag(&dag_name).iter() {
            let (id, val) = iter_elem.unwrap();
            let hash = blake3::Hash::from_bytes((&id as &[u8]).try_into().unwrap());
            let event: Event = deserialize_async(&val).await.unwrap();
            graph.insert(hash, event);
        }

        let mut result = vec![];

        'outer: for tip in tips.iter() {
            for i in tip.1.iter() {
                if !graph.contains_key(i) {
                    continue 'outer;
                }
            }

            for (_, ev) in graph.iter() {
                if ev.layer > *tip.0 && !result.contains(ev) {
                    result.push(ev.clone())
                }
            }
        }

        result.sort_by(|a, b| a.layer.cmp(&b.layer));

        Ok(result)
    }
}
