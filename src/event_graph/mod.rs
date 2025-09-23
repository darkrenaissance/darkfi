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

// use async_std::stream::from_iter;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};

// use futures::stream::FuturesOrdered;
use blake3::Hash;
use darkfi_serial::{deserialize_async, serialize_async};
use event::Header;
use futures::{
    // future,
    stream::FuturesUnordered,
    StreamExt,
};
use num_bigint::BigUint;
use sled_overlay::{sled, SledTreeOverlay};
use smol::{
    lock::{OnceCell, RwLock},
    Executor,
};
use tracing::{debug, error, info, warn};
use url::Url;

use crate::{
    event_graph::util::{next_hour_timestamp, next_rotation_timestamp, replayer_log},
    net::{channel::Channel, P2pPtr},
    system::{msleep, Publisher, PublisherPtr, StoppableTask, StoppableTaskPtr, Subscription},
    Error, Result,
};

#[cfg(feature = "rpc")]
use {
    crate::rpc::{
        jsonrpc::{JsonResponse, JsonResult},
        util::json_map,
    },
    tinyjson::JsonValue::{self},
};

/// An event graph event
pub mod event;
pub use event::Event;

/// P2P protocol implementation for the Event Graph
pub mod proto;
use proto::{EventRep, EventReq, HeaderRep, HeaderReq, TipRep, TipReq};

/// Utility functions
pub mod util;
use util::{generate_genesis, millis_until_next_rotation};

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
pub const NULL_ID: Hash = Hash::from_bytes([0x00; blake3::OUT_LEN]);
/// Null parents
pub const NULL_PARENTS: [Hash; N_EVENT_PARENTS] = [NULL_ID; N_EVENT_PARENTS];

/// Maximum number of DAGs to store, this should be configurable
pub const DAGS_MAX_NUMBER: i8 = 24;

/// Atomic pointer to an [`EventGraph`] instance.
pub type EventGraphPtr = Arc<EventGraph>;
pub type LayerUTips = BTreeMap<u64, HashSet<blake3::Hash>>;

#[derive(Clone)]
pub struct DAGStore {
    db: sled::Db,
    header_dags: HashMap<u64, (sled::Tree, LayerUTips)>,
    main_dags: HashMap<u64, (sled::Tree, LayerUTips)>,
}

impl DAGStore {
    pub async fn new(&self, sled_db: sled::Db, hours_rotation: u64) -> Self {
        let mut considered_trees = HashMap::new();
        let mut considered_header_trees = HashMap::new();
        if hours_rotation > 0 {
            // Create previous genesises if not existing, since they are deterministic.
            for i in 1..=DAGS_MAX_NUMBER {
                let i_hours_ago = next_hour_timestamp((i - DAGS_MAX_NUMBER).into());
                let header = Header {
                    timestamp: i_hours_ago,
                    parents: [NULL_ID; N_EVENT_PARENTS],
                    layer: 0,
                };
                let genesis = Event { header, content: GENESIS_CONTENTS.to_vec() };

                let tree_name = genesis.header.timestamp.to_string();
                let hdr_tree_name = format!("headers_{tree_name}");
                let hdr_dag = sled_db.open_tree(hdr_tree_name).unwrap();
                let dag = sled_db.open_tree(tree_name).unwrap();

                if hdr_dag.is_empty() {
                    let mut overlay = SledTreeOverlay::new(&hdr_dag);

                    let header_se = serialize_async(&genesis.header).await;

                    // Add the header to the overlay
                    overlay.insert(genesis.id().as_bytes(), &header_se).unwrap();

                    // Aggregate changes into a single batch
                    let batch = overlay.aggregate().unwrap();

                    // Atomically apply the batch.
                    // Panic if something is corrupted.
                    if let Err(e) = hdr_dag.apply_batch(batch) {
                        panic!("Failed applying header_dag_insert batch to sled: {}", e);
                    }
                }
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
                considered_header_trees.insert(genesis.header.timestamp, (hdr_dag, utips.clone()));
                considered_trees.insert(genesis.header.timestamp, (dag, utips));
            }
        } else {
            let genesis = generate_genesis(0);
            let tree_name = genesis.header.timestamp.to_string();
            let hdr_tree_name = format!("headers_{tree_name}");
            let hdr_dag = sled_db.open_tree(hdr_tree_name).unwrap();
            let dag = sled_db.open_tree(tree_name).unwrap();
            if hdr_dag.is_empty() {
                let mut overlay = SledTreeOverlay::new(&hdr_dag);

                let header_se = serialize_async(&genesis.header).await;

                // Add the header to the overlay
                overlay.insert(genesis.id().as_bytes(), &header_se).unwrap();

                // Aggregate changes into a single batch
                let batch = overlay.aggregate().unwrap();

                // Atomically apply the batch.
                // Panic if something is corrupted.
                if let Err(e) = hdr_dag.apply_batch(batch) {
                    panic!("Failed applying header_dag_insert batch to sled: {}", e);
                }
            }
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
            considered_header_trees.insert(genesis.header.timestamp, (hdr_dag, utips.clone()));
            considered_trees.insert(genesis.header.timestamp, (dag, utips));
        }

        Self { db: sled_db, header_dags: considered_header_trees, main_dags: considered_trees }
    }

    /// Adds a DAG into the set of DAGs and drops the oldest one if exeeding DAGS_MAX_NUMBER,
    /// This is called if prune_task activates.
    pub async fn add_dag(&mut self, dag_name: &str, genesis_event: &Event) {
        debug!("add_dag::dags: {}", self.main_dags.len());
        if self.main_dags.len() != self.header_dags.len() {
            panic!("main dags length is not the same as header dags")
        }
        // TODO: sort dags by timestamp and drop the oldest
        if self.main_dags.len() > DAGS_MAX_NUMBER.try_into().unwrap() {
            while self.main_dags.len() >= DAGS_MAX_NUMBER.try_into().unwrap() {
                debug!("[EVENTGRAPH] dropping oldest dag");
                let sorted_dags = self.sort_dags().await;
                // since dags are sorted in reverse
                let oldest_tree = sorted_dags.last().unwrap().name();
                let oldest_key = String::from_utf8_lossy(&oldest_tree);
                let oldest_key = u64::from_str(&oldest_key).unwrap();

                let oldest_hdr_tree = self.header_dags.remove(&oldest_key).unwrap();
                let oldest_tree = self.main_dags.remove(&oldest_key).unwrap();
                self.db.drop_tree(oldest_hdr_tree.0.name()).unwrap();
                self.db.drop_tree(oldest_tree.0.name()).unwrap();
            }
        }

        // Insert genesis
        let hdr_tree_name = format!("headers_{dag_name}");
        let hdr_dag = self.get_dag(&hdr_tree_name);
        hdr_dag
            .insert(genesis_event.id().as_bytes(), serialize_async(&genesis_event.header).await)
            .unwrap();

        let dag = self.get_dag(dag_name);
        dag.insert(genesis_event.id().as_bytes(), serialize_async(genesis_event).await).unwrap();
        let utips = self.find_unreferenced_tips(&dag).await;
        self.header_dags.insert(genesis_event.header.timestamp, (hdr_dag, utips.clone()));
        self.main_dags.insert(genesis_event.header.timestamp, (dag, utips));
    }

    // Get a DAG providing its name.
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
            .main_dags
            .iter()
            .map(|x| {
                let trees = x.1;
                trees.0.clone()
            })
            .collect::<Vec<_>>();

        for dag in dags {
            let genesis = dag.first().unwrap().unwrap().1;
            let genesis_event: Event = deserialize_async(&genesis).await.unwrap();
            vec_dags.push((genesis_event.header.timestamp, dag));
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
            for parent in event.header.parents.iter() {
                tips.remove(parent);
            }
        }
        // Build the layers map
        let mut map: LayerUTips = BTreeMap::new();
        for tip in tips {
            let event = self.fetch_event_from_dag(&tip, &dag).await.unwrap().unwrap();
            if let Some(layer_tips) = map.get_mut(&event.header.layer) {
                layer_tips.insert(tip);
            } else {
                let mut layer_tips = HashSet::new();
                layer_tips.insert(tip);
                map.insert(event.header.layer, layer_tips);
            }
        }

        map
    }

    /// Fetch an event from the DAG
    pub async fn fetch_event_from_dag(
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

enum PeerStatus {
    Free,
    Busy,
    Failed,
}

/// An Event Graph instance
pub struct EventGraph {
    /// Pointer to the P2P network instance
    p2p: P2pPtr,
    /// Sled tree containing the headers
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
    broadcasted_ids: RwLock<HashSet<Hash>>,
    /// DAG Pruning Task
    pub prune_task: OnceCell<StoppableTaskPtr>,
    /// Event publisher, this notifies whenever an event is
    /// inserted into the DAG
    pub event_pub: PublisherPtr<Event>,
    /// Current genesis event
    pub current_genesis: RwLock<Event>,
    /// Currently configured DAG rotation, in hours
    hours_rotation: u64,
    /// Flag signalling DAG has finished initial sync
    pub synced: RwLock<bool>,
    /// Enable graph debugging
    pub deg_enabled: RwLock<bool>,
    /// The publisher for which we can give deg info over
    deg_publisher: PublisherPtr<DegEvent>,
    /// Run in replay_mode where if set we log Sled DB instructions
    /// into `datastore`, useful to reacreate a faulty DAG to debug.
    fast_mode: bool,
}

impl EventGraph {
    /// Create a new [`EventGraph`] instance, creates a new Genesis
    /// event and checks if it
    /// is containd in DAG, if not prunes DAG, may also start a pruning
    /// task based on `hours_rotation`, and return an atomic instance of
    /// `Self`
    /// * `p2p` atomic pointer to p2p.
    /// * `sled_db` sled DB instance.
    /// * `datastore` path where we should log db instrucion if run in
    ///   replay mode.
    /// * `replay_mode` set the flag to keep a log of db instructions.
    /// * `hours_rotation` marks the lifetime of the DAG before it's
    ///   pruned.
    pub async fn new(
        p2p: P2pPtr,
        sled_db: sled::Db,
        datastore: PathBuf,
        replay_mode: bool,
        fast_mode: bool,
        hours_rotation: u64,
        ex: Arc<Executor<'_>>,
    ) -> Result<EventGraphPtr> {
        let broadcasted_ids = RwLock::new(HashSet::new());
        let event_pub = Publisher::new();

        // Create the current genesis event based on the `hours_rotation`
        let current_genesis = generate_genesis(hours_rotation);
        let current_dag_tree_name = current_genesis.header.timestamp.to_string();
        let dag_store = DAGStore {
            db: sled_db.clone(),
            header_dags: HashMap::default(),
            main_dags: HashMap::default(),
        }
        .new(sled_db, hours_rotation)
        .await;

        let self_ = Arc::new(Self {
            p2p,
            dag_store: RwLock::new(dag_store.clone()),
            datastore,
            replay_mode,
            fast_mode,
            broadcasted_ids,
            prune_task: OnceCell::new(),
            event_pub,
            current_genesis: RwLock::new(current_genesis.clone()),
            hours_rotation,
            synced: RwLock::new(false),
            deg_enabled: RwLock::new(false),
            deg_publisher: Publisher::new(),
        });

        // Check if we have it in our DAG.
        // If not, we can prune the DAG and insert this new genesis event.
        let dag = dag_store.get_dag(&current_dag_tree_name);
        if !dag.contains_key(current_genesis.id().as_bytes())? {
            info!(
                target: "event_graph::new",
                "[EVENTGRAPH] DAG does not contain current genesis, pruning existing data",
            );
            self_.dag_prune(current_genesis).await?;
        }

        // Spawn the DAG pruning task
        if hours_rotation > 0 {
            let prune_task = StoppableTask::new();
            let _ = self_.prune_task.set(prune_task.clone()).await;

            prune_task.clone().start(
                 self_.clone().dag_prune_task(hours_rotation),
                 |res| async move {
                     match res {
                         Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                         Err(e) => error!(target: "event_graph::_handle_stop", "[EVENTGRAPH] Failed stopping prune task: {e}")
                     }
                 },
                 Error::DetachedTaskStopped,
                 ex.clone(),
             );
        }

        Ok(self_)
    }

    pub fn hours_rotation(&self) -> u64 {
        self.hours_rotation
    }

    /// Sync the DAG from connected peers
    pub async fn dag_sync(&self, dag: sled::Tree, fast_mode: bool) -> Result<()> {
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
            target: "event_graph::dag_sync",
            "[EVENTGRAPH] Syncing DAG from {communicated_peers} peers..."
        );

        let comms_timeout = self.p2p.settings().read().await.outbound_connect_timeout_max();

        // Here we keep track of the tips, their layers and how many time we've seen them.
        let mut tips: HashMap<Hash, (u64, usize)> = HashMap::new();

        // Let's first ask all of our peers for their tips and collect them
        // in our hashmap above.
        for channel in channels.iter() {
            let url = channel.display_address();

            let tip_rep_sub = match channel.subscribe_msg::<TipRep>().await {
                Ok(v) => v,
                Err(e) => {
                    error!(
                        target: "event_graph::dag_sync",
                        "[EVENTGRAPH] Sync: Couldn't subscribe TipReq for peer {url}, skipping ({e})"
                    );
                    communicated_peers -= 1;
                    continue
                }
            };

            if let Err(e) = channel.send(&TipReq(dag_name.clone())).await {
                error!(
                    target: "event_graph::dag_sync",
                    "[EVENTGRAPH] Sync: Couldn't contact peer {url}, skipping ({e})"
                );
                communicated_peers -= 1;
                continue
            };

            // Node waits for response
            let Ok(peer_tips) = tip_rep_sub.receive_with_timeout(comms_timeout).await else {
                error!(
                    target: "event_graph::dag_sync",
                    "[EVENTGRAPH] Sync: Peer {url} didn't reply with tips in time, skipping"
                );
                communicated_peers -= 1;
                continue
            };

            let peer_tips: &BTreeMap<u64, HashSet<Hash>> = &peer_tips.0;

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
                target: "event_graph::dag_sync",
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

        if fast_mode {
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
                info!(target: "event_graph::dag_sync", "[EVENTGRAPH] DAG synced successfully!");
                return Ok(())
            }
        }

        // Header sync first
        // TODO: requesting headers should be in a way that we wouldn't
        // recieve the same header(s) again, by sending our tip, other
        // nodes should send back the ones after it
        let hdr_tree_name = format!("headers_{dag_name}");
        let header_dag = self.dag_store.read().await.get_dag(&hdr_tree_name);
        let mut headers_requests = FuturesUnordered::new();
        for channel in channels.iter() {
            headers_requests.push(request_header(&channel, dag_name.clone(), comms_timeout))
        }

        while let Some(peer_headers) = headers_requests.next().await {
            self.header_dag_insert(peer_headers?, &dag_name).await?
        }

        // start download payload
        if !fast_mode {
            info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] Fetching events");
            let mut header_sorted = vec![];

            for iter_elem in header_dag.iter() {
                let (_, val) = iter_elem.unwrap();
                let val: Header = deserialize_async(&val).await.unwrap();
                if val.parents != NULL_PARENTS {
                    header_sorted.push(val);
                }
            }
            header_sorted.sort_by(|x, y| y.layer.cmp(&x.layer));

            info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] Retrieving {} Events", header_sorted.len());
            // Implement parallel download of events with a batch size
            let batch = 20;
            // Mapping of the chunk group id to the chunk, using a BTreeMap help us to
            // prioritize the older headers when our request fails and we retry
            let mut chunks: BTreeMap<usize, Vec<blake3::Hash>> = BTreeMap::new();
            for (i, chunk) in header_sorted.chunks(batch).enumerate() {
                chunks.insert(i, chunk.iter().map(|h| h.id()).collect());
            }
            let mut remaining_chunk_ids: BTreeSet<usize> = chunks.keys().cloned().collect();

            // Mapping of the chunk group id to the received events, using a BTreeMap help
            // us to verify and insert the events in order
            let mut received_events: BTreeMap<usize, Vec<Event>> = BTreeMap::new();
            // Track peers status so that we don't send a new request to the same peer before they
            // finish the first or send to a failed peer
            let mut peer_status: HashMap<Url, PeerStatus> = HashMap::new();

            let mut retrieved_count = 0;
            let mut futures = FuturesUnordered::new();

            while retrieved_count < header_sorted.len() {
                // Retrieve peers in each loop so we don't send requests to a closed channel
                let mut free_channels = vec![];
                let mut busy_channels = 0;

                self.p2p.hosts().peers().iter().for_each(|channel| {
                    if let Some(status) = peer_status.get(channel.address()) {
                        match status {
                            PeerStatus::Free => free_channels.push(channel.clone()),
                            PeerStatus::Busy => busy_channels += 1,
                            _ => {}
                        }
                    } else {
                        peer_status.insert(channel.address().clone(), PeerStatus::Free);
                        free_channels.push(channel.clone());
                    }
                });

                // We don't have any channels we can assign to or wait to get response from
                if free_channels.is_empty() && busy_channels == 0 {
                    return Err(Error::DagSyncFailed);
                }

                // We will distribute the remaining chunks to each channel
                let requested_chunks_len =
                    std::cmp::min(free_channels.len(), remaining_chunk_ids.len());
                let requested_chunk_ids: Vec<usize> =
                    remaining_chunk_ids.iter().take(requested_chunks_len).copied().collect();
                for (i, chunk_id) in requested_chunk_ids.iter().enumerate() {
                    futures.push(request_event(
                        free_channels[i].clone(),
                        chunks.get(chunk_id).unwrap().clone(),
                        *chunk_id,
                        comms_timeout,
                    ));
                    remaining_chunk_ids.remove(chunk_id);
                    peer_status.insert(free_channels[i].address().clone(), PeerStatus::Busy);
                }

                info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] Retrieving Events from {} peers", futures.len());
                if let Some(resp) = futures.next().await {
                    let (events, chunk_id, channel) = resp;
                    if let Ok(events) = events {
                        retrieved_count += events.len();
                        received_events.insert(chunk_id, events.clone());
                        peer_status.insert(channel.address().clone(), PeerStatus::Free);
                    } else {
                        remaining_chunk_ids.insert(chunk_id);
                        peer_status.insert(channel.address().clone(), PeerStatus::Failed);
                    }

                    info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] Retrieved Events: {}/{}", retrieved_count, header_sorted.len());
                }
            }

            let mut verified_count = 0;
            for (_, chunk) in received_events {
                verified_count += chunk.len();
                self.dag_insert(&chunk, &dag_name).await?;
                info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] Verified Events: {}/{}", verified_count, retrieved_count);
            }
        }
        // <-- end download payload

        *self.synced.write().await = true;

        info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] DAG synced successfully!");
        Ok(())
    }

    /// Choose how many dags to sync
    pub async fn sync_selected(&self, count: usize, fast_mode: bool) -> Result<()> {
        let mut dags_to_sync = self.dag_store.read().await.get_dags(count).await;
        // Since get_dags() return sorted dags in reverse
        dags_to_sync.reverse();
        for dag in dags_to_sync {
            match self.dag_sync(dag, fast_mode).await {
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
        debug!(target: "event_graph::dag_prune", "Pruning DAG...");

        // Acquire exclusive locks to unreferenced_tips, broadcasted_ids and
        // current_genesis while this operation is happening. We do this to
        // ensure that during the pruning operation, no other operations are
        // able to access the intermediate state which could lead to producing
        // the wrong state after pruning.
        let mut broadcasted_ids = self.broadcasted_ids.write().await;
        let mut current_genesis = self.current_genesis.write().await;

        let dag_name = genesis_event.header.timestamp.to_string();
        self.dag_store.write().await.add_dag(&dag_name, &genesis_event).await;

        // Clear bcast ids
        *current_genesis = genesis_event;
        *broadcasted_ids = HashSet::new();
        drop(broadcasted_ids);
        drop(current_genesis);

        debug!(target: "event_graph::dag_prune", "DAG pruned successfully");
        Ok(())
    }

    /// Background task periodically pruning the DAG.
    async fn dag_prune_task(self: Arc<Self>, hours_rotation: u64) -> Result<()> {
        // The DAG should periodically be pruned. This can be a configurable
        // parameter. By pruning, we should deterministically replace the
        // genesis event (can use a deterministic timestamp) and drop everything
        // in the DAG, leaving just the new genesis event.
        debug!(target: "event_graph::dag_prune_task", "Spawned background DAG pruning task");

        loop {
            // Find the next rotation timestamp:
            let next_rotation = next_rotation_timestamp(INITIAL_GENESIS, hours_rotation);

            let header =
                Header { timestamp: next_rotation, parents: [NULL_ID; N_EVENT_PARENTS], layer: 0 };
            // Prepare the new genesis event
            let current_genesis = Event { header, content: GENESIS_CONTENTS.to_vec() };

            // Sleep until it's time to rotate.
            let s = millis_until_next_rotation(next_rotation);

            debug!(target: "event_graph::dag_prune_task", "Sleeping {s}ms until next DAG prune");
            msleep(s).await;
            debug!(target: "event_graph::dag_prune_task", "Rotation period reached");

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
    pub async fn dag_insert(&self, events: &[Event], dag_name: &str) -> Result<Vec<Hash>> {
        // Sanity check
        if events.is_empty() {
            return Ok(vec![])
        }

        // Acquire exclusive locks to `broadcasted_ids`
        let dag_timestamp = u64::from_str(dag_name).unwrap();
        let mut broadcasted_ids = self.broadcasted_ids.write().await;

        let main_dag = self.dag_store.read().await.get_dag(dag_name);
        let hdr_tree_name = format!("headers_{dag_name}");
        let header_dag = self.dag_store.read().await.get_dag(&hdr_tree_name);

        // Here we keep the IDs to return
        let mut ids = Vec::with_capacity(events.len());

        // Create an overlay over the DAG tree
        let mut overlay = SledTreeOverlay::new(&main_dag);

        // Iterate over given events to validate them and
        // write them to the overlay
        for event in events {
            let event_id = event.id();
            if event.header.parents == NULL_PARENTS {
                break
            }
            debug!(
                target: "event_graph::dag_insert",
                "Inserting event {event_id} into the DAG layer: {}", event.header.layer
            );

            // check if we already have the event
            if main_dag.contains_key(event_id.as_bytes())? {
                continue
            }

            // check if its header is in header's store
            if !header_dag.contains_key(event_id.as_bytes())? {
                continue
            }

            if !event.dag_validate(&header_dag).await? {
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
        let batch = match overlay.aggregate() {
            Some(x) => x,
            None => return Ok(vec![]),
        };

        // Atomically apply the batch.
        // Panic if something is corrupted.
        if let Err(e) = main_dag.apply_batch(batch) {
            panic!("Failed applying dag_insert batch to sled: {e}");
        }

        let mut dag_store = self.dag_store.write().await;
        let (_, unreferenced_tips) = &mut dag_store.main_dags.get_mut(&dag_timestamp).unwrap();

        // Iterate over given events to update references and
        // send out notifications about them
        for event in events {
            let event_id = event.id();

            // Update the unreferenced DAG tips set
            debug!(
                target: "event_graph::dag_insert",
                "Event {event_id} parents {:#?}", event.header.parents,
            );
            for parent_id in event.header.parents.iter() {
                if parent_id != &NULL_ID {
                    debug!(
                        target: "event_graph::dag_insert",
                        "Removing {parent_id} from unreferenced_tips"
                    );

                    // Iterate over unreferenced tips in previous layers
                    // and remove the parent
                    // NOTE: this might be too exhaustive, but the
                    // assumption is that previous layers unreferenced
                    // tips will be few.
                    for (layer, tips) in unreferenced_tips.iter_mut() {
                        if layer >= &event.header.layer {
                            continue
                        }
                        tips.remove(parent_id);
                    }
                    broadcasted_ids.insert(*parent_id);
                }
            }
            unreferenced_tips.retain(|_, tips| !tips.is_empty());
            debug!(
                target: "event_graph::dag_insert",
                "Adding {event_id} to unreferenced tips"
            );

            if let Some(layer_tips) = unreferenced_tips.get_mut(&event.header.layer) {
                layer_tips.insert(event_id);
            } else {
                let mut layer_tips = HashSet::new();
                layer_tips.insert(event_id);
                unreferenced_tips.insert(event.header.layer, layer_tips);
            }

            // Send out notifications about the new event
            self.event_pub.notify(event.clone()).await;
        }

        dag_store.header_dags.get_mut(&dag_timestamp).unwrap().1 =
            dag_store.main_dags.get(&dag_timestamp).unwrap().1.clone();

        // Drop the exclusive locks
        drop(dag_store);
        drop(broadcasted_ids);

        Ok(ids)
    }

    pub async fn header_dag_insert(&self, headers: Vec<Header>, dag_name: &str) -> Result<()> {
        let hdr_tree_name = format!("headers_{dag_name}");
        let header_dag = self.dag_store.read().await.get_dag(&hdr_tree_name);
        // Create an overlay over the DAG tree
        let mut overlay = SledTreeOverlay::new(&header_dag);

        let mut hdrs = headers;
        hdrs.sort_by(|x, y| x.layer.cmp(&y.layer));

        // Iterate over given events to validate them and
        // write them to the overlay
        for header in hdrs {
            let header_id = header.id();
            if header.parents == NULL_PARENTS {
                continue
            }
            debug!(
                target: "event_graph::header_dag_insert()",
                "Inserting header {} into the DAG", header_id,
            );
            if !header.validate(&header_dag, self.hours_rotation, Some(&overlay)).await? {
                error!(target: "event_graph::header_dag_insert()", "Header {} is invalid!", header_id);
                return Err(Error::HeaderIsInvalid)
            }
            let header_se = serialize_async(&header).await;

            // Add the event to the overlay
            overlay.insert(header_id.as_bytes(), &header_se)?;
        }

        // Aggregate changes into a single batch
        let batch = match overlay.aggregate() {
            Some(x) => x,
            None => return Ok(()),
        };

        // Atomically apply the batch.
        // Panic if something is corrupted.
        if let Err(e) = header_dag.apply_batch(batch) {
            panic!("Failed applying dag_insert batch to sled: {}", e);
        }

        Ok(())
    }

    /// Search and fetch an event through all DAGs
    pub async fn fetch_event_from_dags(&self, event_id: &blake3::Hash) -> Result<Option<Event>> {
        let store = self.dag_store.read().await;
        for tree_elem in store.main_dags.clone() {
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
        dag_name: &u64,
    ) -> (u64, [blake3::Hash; N_EVENT_PARENTS]) {
        let store = self.dag_store.read().await;
        let (_, unreferenced_tips) = store.header_dags.get(dag_name).unwrap();

        let mut parents = [NULL_ID; N_EVENT_PARENTS];
        let mut index = 0;
        'outer: for (_, tips) in unreferenced_tips.iter().rev() {
            for tip in tips.iter() {
                parents[index] = *tip;
                index += 1;
                if index >= N_EVENT_PARENTS {
                    break 'outer;
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
        for (i, _) in self.dag_store.read().await.header_dags.iter() {
            let (_, tips) = self.get_next_layer_with_parents(&i).await;
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
                    let tip = self.fetch_event_from_dags(&tip).await.unwrap().unwrap();
                    ordered_events.extend(self.dfs_topological_sort(tip, &mut visited).await);
                }
            }
        }

        let mut ord_events_vec = ordered_events.make_contiguous().to_vec();
        // Order events by timestamp.
        ord_events_vec.sort_unstable_by(|a, b| a.1.header.timestamp.cmp(&b.1.header.timestamp));

        ord_events_vec.iter().map(|a| a.1.clone()).collect::<Vec<Event>>()
    }

    /// We do a non-recursive DFS (<https://en.wikipedia.org/wiki/Depth-first_search>),
    /// and additionally we consider the timestamps.
    async fn dfs_topological_sort(
        &self,
        event: Event,
        visited: &mut HashSet<Hash>,
    ) -> VecDeque<(u64, Event)> {
        let mut ordered_events = VecDeque::new();
        let mut stack = VecDeque::new();
        let event_id = event.id();
        stack.push_back(event_id);

        while let Some(event_id) = stack.pop_front() {
            if !visited.contains(&event_id) && event_id != NULL_ID {
                visited.insert(event_id);
                if let Some(event) = self.fetch_event_from_dags(&event_id).await.unwrap() {
                    for parent in event.header.parents.iter() {
                        stack.push_back(*parent);
                    }

                    ordered_events.push_back((event.header.layer, event))
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

    #[cfg(feature = "rpc")]
    pub async fn eventgraph_info(&self, id: u16, _params: JsonValue) -> JsonResult {
        let current_genesis = self.current_genesis.read().await;
        let dag_name = current_genesis.header.timestamp.to_string();
        let mut graph = HashMap::new();
        for iter_elem in self.dag_store.read().await.get_dag(&dag_name).iter() {
            let (id, val) = iter_elem.unwrap();
            let id = Hash::from_bytes((&id as &[u8]).try_into().unwrap());
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
             target: "event_graph::fetch_successors_of",
             "fetching successors of {tips:?}"
        );

        let current_genesis = self.current_genesis.read().await;
        let dag_name = current_genesis.header.timestamp.to_string();
        let mut graph = HashMap::new();
        for iter_elem in self.dag_store.read().await.get_dag(&dag_name).iter() {
            let (id, val) = iter_elem.unwrap();
            let hash = Hash::from_bytes((&id as &[u8]).try_into().unwrap());
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
                if ev.header.layer > *tip.0 && !result.contains(ev) {
                    result.push(ev.clone())
                }
            }
        }

        result.sort_by(|a, b| a.header.layer.cmp(&b.header.layer));

        Ok(result)
    }
}

async fn request_header(
    peer: &Channel,
    tree_name: String,
    comms_timeout: u64,
) -> Result<Vec<Header>> {
    let url = peer.address();

    let hdr_rep_sub = match peer.subscribe_msg::<HeaderRep>().await {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "event_graph::dag_sync()",
                "[EVENTGRAPH] Sync: Couldn't subscribe HeaderReq for peer {}, skipping ({})",
                url, e,
            );
            return Err(Error::EventNotFound("Couldn't subscribe HeaderReq".to_owned()));
        }
    };

    if let Err(e) = peer.send(&HeaderReq(tree_name)).await {
        error!(
            target: "event_graph::dag_sync()",
            "[EVENTGRAPH] Sync: Couldn't contact peer {}, skipping ({})", url, e,
        );
        return Err(Error::EventNotFound("Couldn't contact peer".to_owned()));
    };

    // Node waits for response
    let Ok(peer_headers) = hdr_rep_sub.receive_with_timeout(comms_timeout).await else {
        error!(
            target: "event_graph::dag_sync()",
            "[EVENTGRAPH] Sync: Peer {} didn't reply with headers in time, skipping", url,
        );
        // communicated_peers -= 1;
        return Err(Error::EventNotFound("Peer didn't reply with headers in time".to_owned()));
    };

    let peer_headers = &peer_headers.0;
    Ok(peer_headers.to_vec())
}

async fn request_event(
    peer: Arc<Channel>,
    headers: Vec<Hash>,
    chunk_id: usize,
    comms_timeout: u64,
) -> (Result<Vec<Event>>, usize, Arc<Channel>) {
    let url = peer.address();

    debug!(
        target: "event_graph::dag_sync()",
        "Requesting {:?} from {}...", headers, url,
    );

    let ev_rep_sub = match peer.subscribe_msg::<EventRep>().await {
        Ok(v) => v,
        Err(e) => {
            error!(
                target: "event_graph::dag_sync()",
                "[EVENTGRAPH] Sync: Couldn't subscribe EventRep for peer {}, skipping ({})",
                url, e,
            );
            return (
                Err(Error::EventNotFound("Couldn't subscribe EventRep".to_owned())),
                chunk_id,
                peer,
            );
        }
    };

    // let request_missing_events = missing_parents.clone().into_iter().collect();
    if let Err(e) = peer.send(&EventReq(headers.clone())).await {
        error!(
            target: "event_graph::dag_sync()",
            "[EVENTGRAPH] Sync: Failed communicating EventReq({:?}) to {}: {}",
            headers, url, e,
        );
        return (
            Err(Error::EventNotFound("Failed communicating EventReq".to_owned())),
            chunk_id,
            peer,
        );
    }

    // Node waits for response
    let Ok(event) = ev_rep_sub.receive_with_timeout(comms_timeout).await else {
        error!(
            target: "event_graph::dag_sync()",
            "[EVENTGRAPH] Sync: Timeout waiting for parents {:?} from {}",
            headers, url,
        );
        return (
            Err(Error::EventNotFound("Timeout waiting for parents".to_owned())),
            chunk_id,
            peer,
        );
    };

    (Ok(event.0.clone()), chunk_id, peer)
}
