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
use futures::{
    // future,
    stream::{FuturesOrdered, FuturesUnordered},
    StreamExt,
};
use rand::{rngs::OsRng, seq::SliceRandom};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::Arc,
};

use blake3::Hash;
use darkfi_serial::{deserialize_async, serialize_async};
use futures::future::join_all;
use event::Header;
use num_bigint::BigUint;
use sled_overlay::{sled, SledTreeOverlay};
use smol::{
    lock::{OnceCell, RwLock},
    Executor,
};
use tracing::{debug, error, info, warn};

use crate::{
    event_graph::util::replayer_log,
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
use util::{generate_genesis, millis_until_next_rotation, next_rotation_timestamp};

// Debugging event graph
pub mod deg;
use deg::DegEvent;
use crate::net::ChannelPtr;

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

/// Atomic pointer to an [`EventGraph`] instance.
pub type EventGraphPtr = Arc<EventGraph>;

/// An Event Graph instance
pub struct EventGraph {
    /// Pointer to the P2P network instance
    p2p: P2pPtr,
    /// Sled tree containing the headers
    header_dag: sled::Tree,
    /// Main sled tree containing the events
    main_dag: sled::Tree,
    /// Replay logs path.
    datastore: PathBuf,
    /// Run in replay_mode where if set we log Sled DB instructions
    /// into `datastore`, useful to reacreate a faulty DAG to debug.
    replay_mode: bool,
    /// The set of unreferenced DAG tips
    unreferenced_tips: RwLock<BTreeMap<u64, HashSet<Hash>>>,
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
    current_genesis: RwLock<Event>,
    /// Currently configured DAG rotation, in days
    days_rotation: u64,
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
    /// task based on `days_rotation`, and return an atomic instance of
    /// `Self`
    /// * `p2p` atomic pointer to p2p.
    /// * `sled_db` sled DB instance.
    /// * `datastore` path where we should log db instrucion if run in
    ///   replay mode.
    /// * `replay_mode` set the flag to keep a log of db instructions.
    /// * `dag_tree_name` the name of disk-backed tree (or DAG name).
    /// * `days_rotation` marks the lifetime of the DAG before it's
    ///   pruned.
    pub async fn new(
        p2p: P2pPtr,
        sled_db: sled::Db,
        datastore: PathBuf,
        replay_mode: bool,
        fast_mode: bool,
        dag_tree_name: &str,
        days_rotation: u64,
        ex: Arc<Executor<'_>>,
    ) -> Result<EventGraphPtr> {
        let hdr_tree_name = format!("headers_{dag_tree_name}");
        let hdr_dag = sled_db.open_tree(hdr_tree_name)?;
        let dag = sled_db.open_tree(dag_tree_name)?;
        let unreferenced_tips = RwLock::new(BTreeMap::new());
        let broadcasted_ids = RwLock::new(HashSet::new());
        let event_pub = Publisher::new();

        // Create the current genesis event based on the `days_rotation`
        let current_genesis = generate_genesis(days_rotation);
        let self_ = Arc::new(Self {
            p2p,
            header_dag: hdr_dag.clone(),
            main_dag: dag.clone(),
            datastore,
            replay_mode,
            fast_mode,
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
        if !dag.contains_key(current_genesis.header.id().as_bytes())? {
            info!(
                target: "event_graph::new",
                "[EVENTGRAPH] DAG does not contain current genesis, pruning existing data",
            );
            self_.dag_prune(current_genesis).await?;
        }

        // Find the unreferenced tips in the current DAG state.
        *self_.unreferenced_tips.write().await = self_.find_unreferenced_tips().await;

        // Spawn the DAG pruning task
        if days_rotation > 0 {
            let prune_task = StoppableTask::new();
            let _ = self_.prune_task.set(prune_task.clone()).await;

            prune_task.clone().start(
                 self_.clone().dag_prune_task(days_rotation),
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

    pub fn days_rotation(&self) -> u64 {
        self.days_rotation
    }

    /// Sync the DAG from connected peers
    pub async fn dag_sync(&self, fast_mode: bool) -> Result<()> {
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

            if let Err(e) = channel.send(&TipReq {}).await {
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

                if !self.main_dag.contains_key(tip.as_bytes()).unwrap() {
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
        let mut headers_requests = FuturesUnordered::new();
        for channel in channels.iter() {
            headers_requests.push(request_header(&channel, comms_timeout))
        }

        while let Some(peer_headers) = headers_requests.next().await {
            self.header_dag_insert(peer_headers?).await?
        }

        // start download payload
        if !fast_mode {
            info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] Fetching events");
            let mut header_sorted = vec![];

            for iter_elem in self.header_dag.iter() {
                let (_, val) = iter_elem.unwrap();
                let val: Header = deserialize_async(&val).await.unwrap();
                header_sorted.push(val);
            }
            header_sorted.sort_by(|x, y| y.layer.cmp(&x.layer));

            info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] Retrieving {} Events", header_sorted.len());
            // Implement parallel download of events with a batch size
            let batch = 20;
            // Mapping of the chunk group id to the chunk, using a BTreeMap help us to
            // prioritize the older headers when our request fails and we retry
            let mut remaining_chunks: BTreeMap<usize, Vec<blake3::Hash>> = BTreeMap::new();
            for (i, chunk) in header_sorted.chunks(batch).enumerate() {
                remaining_chunks.insert(i, chunk.iter().map(|h| h.id()).collect());
            }

            // Mapping of the chunk group id to the received events, using a BTreeMap help
            // us to verify and insert the events in order
            let mut received_events: BTreeMap<usize, Vec<Event>> = BTreeMap::new();
            // Track peers that failed us so we don't send request again
            let mut failed_peers = vec![];
            let mut retrieved_count = 0;

            while remaining_chunks.len() > 0 {

                // Retrieve peers in each loop so we don't send requests to a closed channel
                let channels: Vec<ChannelPtr> = self.p2p.hosts().peers().iter().filter(|c| !failed_peers.contains(c.address())).cloned().collect();

                if channels.len() == 0 {
                    // Removing peers that failed us might be too strict but it is better than
                    // looping over failed peers knowing that they may never provide the event.
                    // Also the DAG sync is retried so it is not a problem.
                    return Err(Error::DagSyncFailed);
                }


                // We will distribute the remaining chunks to each channel
                let requested_chunks_len = std::cmp::min(channels.len(), remaining_chunks.len());
                let keys : Vec<_> = remaining_chunks.keys().take(requested_chunks_len).cloned().collect();
                let mut requested_chunk_ids = Vec::with_capacity(requested_chunks_len);
                let mut requested_chunks = Vec::with_capacity(requested_chunks_len);
                let mut futures = vec![];

                for (i, key) in keys.iter().enumerate() {
                    if let Some(value) = remaining_chunks.remove(&key) {
                        requested_chunk_ids.push(*key);
                        requested_chunks.push(value.clone());
                        futures.push(request_event(channels[i].clone(), value, comms_timeout));
                    }
                }

                info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] Retrieving Events from {} peers", futures.len());
                let rets = join_all(futures).await;

                for (i, res) in rets.iter().enumerate() {
                    if let Ok(events) = res {
                        retrieved_count += events.len();
                        received_events.insert(requested_chunk_ids[i], events.clone());
                    } else {
                        // The request has failed so insert the chunks back to remaining to try with another peer
                        // also note the peer so we don't ask again
                        remaining_chunks.insert(requested_chunk_ids[i], requested_chunks[i].clone());
                        failed_peers.push(channels[i].address().clone());
                    }
                }

                info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] Retrieved Events: {}/{}", retrieved_count, header_sorted.len());
            }

            let mut verified_count = 0;
            for (_, chunk) in received_events {
                verified_count += chunk.len();
                self.dag_insert(&chunk).await?;
                info!(target: "event_graph::dag_sync()", "[EVENTGRAPH] Verified Events: {}/{}", verified_count, retrieved_count);
            }

            // 1. Fetch events one by one
            // let mut events_requests = FuturesOrdered::new();
            // let peer = peer_selection(peers.clone());
            // let peer = channels[0].clone();
            // for header in header_sorted.iter() {
            //    let received_events =
            //        request_event(peer.clone(), vec![header.id()], comms_timeout).await?;
            //    self.dag_insert(&received_events).await?;
            //}

            // let mut received_events = vec![];
            // while let Some(peer_events) = events_requests.next().await {
            //     let events = peer_events?;
            //     for i in events.iter() {
            //         info!("Received events id: {:?}", i.header.id());
            //         info!("layer: {}", i.header.layer);
            //     }
            //     received_events.extend(events);
            // }

            // self.dag_insert(&received_events).await?;

            // // 2. split sorted headers into chunks and assign them to each connected peer
            // let mut responses = vec![];
            // for header in header_sorted.chunks_exact(peers.len()) {
            //     // For each peer, create a future that sends a request
            //     let pairs = peers.iter().zip(header).collect::<Vec<_>>();
            //     let pair_stream = from_iter(pairs.iter());
            //     let requests_stream = pair_stream.map(|(peer, header)| send_request(peer, header));
            //     // Collect all the responses into a vector
            //     let x = requests_stream.collect::<Vec<_>>().await;
            //     info!("len of x: {}", x.len());
            //     // responses.push(x);
            //     responses.extend(x);
            // }
            // // Wait for all the futures to complete
            // let x = future::join_all(responses).await;
            // let fetched_parents = x.into_iter().map(|f| f.unwrap()).collect::<Vec<_>>().concat();
            // info!("fetched parents: {}", fetched_parents.len());
            // for i in fetched_parents.iter() {
            //     info!("layer: {}", i.header.layer)
            // }

            // // 3. Fetch all events at once (just a POC)
            // let peers = channels.clone().into_iter().collect::<Vec<_>>();
            // let missing = header_sorted.iter().map(|x| x.id()).collect::<Vec<_>>();
            // info!("first missing: {}", missing[0]);
            // let parents = send_requests(&peers, &missing).await?.concat();
            // info!("fetched parents: {}", parents.len());
        }
        // <-- end download payload

        *self.synced.write().await = true;

        info!(target: "event_graph::dag_sync", "[EVENTGRAPH] DAG synced successfully!");
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
        let mut unreferenced_tips = self.unreferenced_tips.write().await;
        let mut broadcasted_ids = self.broadcasted_ids.write().await;
        let mut current_genesis = self.current_genesis.write().await;

        // Atomically clear the main and headers DAGs and write the new genesis event.
        // Header
        let mut batch = sled::Batch::default();
        for key in self.header_dag.iter().keys() {
            batch.remove(key.unwrap());
        }
        batch.insert(
            genesis_event.header.id().as_bytes(),
            serialize_async(&genesis_event.header).await,
        );

        debug!(target: "event_graph::dag_prune", "Applying header batch...");
        if let Err(e) = self.header_dag.apply_batch(batch) {
            panic!("Failed pruning header DAG, sled apply_batch error: {}", e);
        }

        // Main
        let mut batch = sled::Batch::default();
        for key in self.main_dag.iter().keys() {
            batch.remove(key.unwrap());
        }
        batch.insert(genesis_event.header.id().as_bytes(), serialize_async(&genesis_event).await);

        debug!(target: "event_graph::dag_prune", "Applying main batch...");
        if let Err(e) = self.main_dag.apply_batch(batch) {
            panic!("Failed pruning main DAG, sled apply_batch error: {e}");
        }

        // Clear unreferenced tips and bcast ids
        *unreferenced_tips = BTreeMap::new();
        unreferenced_tips.insert(0, HashSet::from([genesis_event.header.id()]));
        *current_genesis = genesis_event;
        *broadcasted_ids = HashSet::new();
        drop(unreferenced_tips);
        drop(broadcasted_ids);
        drop(current_genesis);

        debug!(target: "event_graph::dag_prune", "DAG pruned successfully");
        Ok(())
    }

    /// Background task periodically pruning the DAG.
    async fn dag_prune_task(self: Arc<Self>, days_rotation: u64) -> Result<()> {
        // The DAG should periodically be pruned. This can be a configurable
        // parameter. By pruning, we should deterministically replace the
        // genesis event (can use a deterministic timestamp) and drop everything
        // in the DAG, leaving just the new genesis event.
        debug!(target: "event_graph::dag_prune_task", "Spawned background DAG pruning task");

        loop {
            // Find the next rotation timestamp:
            let next_rotation = next_rotation_timestamp(INITIAL_GENESIS, days_rotation);

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
    pub async fn dag_insert(&self, events: &[Event]) -> Result<Vec<Hash>> {
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
        let mut overlay = SledTreeOverlay::new(&self.main_dag);

        // Grab genesis timestamp
        // let genesis_timestamp = self.current_genesis.read().await.header.timestamp;

        // Iterate over given events to validate them and
        // write them to the overlay
        for event in events {
            let event_id = event.header.id();
            if event.header.layer == 0 {
                return Ok(vec![])
            }
            debug!(
                target: "event_graph::dag_insert",
                "Inserting event {event_id} into the DAG layer: {}", event.header.layer
            );

            // check if we already have the event
            if self.main_dag.contains_key(event_id.as_bytes())? {
                continue
            }

            // check if its header is in header's store
            if !self.header_dag.contains_key(event_id.as_bytes())? {
                continue
            }

            if !event.validate(&self.header_dag).await? {
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
        if let Err(e) = self.main_dag.apply_batch(batch) {
            panic!("Failed applying dag_insert batch to sled: {e}");
        }

        // Iterate over given events to update references and
        // send out notifications about them
        for event in events {
            let event_id = event.header.id();

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

        // Drop the exclusive locks
        drop(unreferenced_tips);
        drop(broadcasted_ids);

        Ok(ids)
    }

    pub async fn header_dag_insert(&self, headers: Vec<Header>) -> Result<()> {
        // Create an overlay over the DAG tree
        let mut overlay = SledTreeOverlay::new(&self.header_dag);

        // Grab genesis timestamp
        let genesis_timestamp = self.current_genesis.read().await.header.timestamp;

        // Acquire exclusive locks to `unreferenced_tips and broadcasted_ids`
        // let mut unreferenced_header = self.unreferenced_tips.write().await;
        // let mut broadcasted_ids = self.broadcasted_ids.write().await;

        let mut hdrs = headers;
        hdrs.sort_by(|x, y| x.layer.cmp(&y.layer));

        // Iterate over given events to validate them and
        // write them to the overlay
        for header in hdrs {
            let header_id = header.id();
            if header.layer == 0 {
                continue
            }
            debug!(
                target: "event_graph::header_dag_insert()",
                "Inserting header {} into the DAG", header_id,
            );
            if !header
                .validate(&self.header_dag, genesis_timestamp, self.days_rotation, Some(&overlay))
                .await?
            {
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
        if let Err(e) = self.header_dag.apply_batch(batch) {
            panic!("Failed applying dag_insert batch to sled: {}", e);
        }

        Ok(())
    }

    /// Fetch an event from the DAG
    pub async fn dag_get(&self, event_id: &Hash) -> Result<Option<Event>> {
        let Some(bytes) = self.main_dag.get(event_id.as_bytes())? else { return Ok(None) };
        let event: Event = deserialize_async(&bytes).await?;

        Ok(Some(event))
    }

    /// Get next layer along with its N_EVENT_PARENTS from the unreferenced
    /// tips of the DAG. Since tips are mapped by their layer, we go backwards
    /// until we fill the vector, ensuring we always use latest layers tips as
    /// parents.
    async fn get_next_layer_with_parents(&self) -> (u64, [Hash; N_EVENT_PARENTS]) {
        let unreferenced_tips = self.unreferenced_tips.read().await;

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

    /// Find the unreferenced tips in the current DAG state, mapped by their layers.
    async fn find_unreferenced_tips(&self) -> BTreeMap<u64, HashSet<Hash>> {
        // First get all the event IDs
        let mut tips = HashSet::new();
        for iter_elem in self.main_dag.iter() {
            let (id, _) = iter_elem.unwrap();
            let id = Hash::from_bytes((&id as &[u8]).try_into().unwrap());
            tips.insert(id);
        }

        // Iterate again to find unreferenced IDs
        for iter_elem in self.main_dag.iter() {
            let (_, event) = iter_elem.unwrap();
            let event: Event = deserialize_async(&event).await.unwrap();
            for parent in event.header.parents.iter() {
                tips.remove(parent);
            }
        }

        // Build the layers map
        let mut map: BTreeMap<u64, HashSet<Hash>> = BTreeMap::new();
        for tip in tips {
            let event = self.dag_get(&tip).await.unwrap().unwrap();
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

    /// Internal function used for DAG sorting.
    async fn get_unreferenced_tips_sorted(&self) -> [Hash; N_EVENT_PARENTS] {
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

            tips_sorted[i] = Hash::from_bytes(bytes.try_into().unwrap());
        }

        tips_sorted
    }

    /// Perform a topological sort of the DAG.
    pub async fn order_events(&self) -> Vec<Event> {
        let mut ordered_events = VecDeque::new();
        let mut visited = HashSet::new();

        for tip in self.get_unreferenced_tips_sorted().await {
            if !visited.contains(&tip) && tip != NULL_ID {
                let tip = self.dag_get(&tip).await.unwrap().unwrap();
                ordered_events.extend(self.dfs_topological_sort(tip, &mut visited).await);
            }
        }

        let mut ord_events_vec = ordered_events.make_contiguous().to_vec();
        // Order events based on thier layer numbers, or based on timestamp if they are equal
        ord_events_vec.sort_unstable_by(|a, b| {
            a.0.cmp(&b.0).then(b.1.header.timestamp.cmp(&a.1.header.timestamp))
        });

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
        let event_id = event.header.id();
        stack.push_back(event_id);

        while let Some(event_id) = stack.pop_front() {
            if !visited.contains(&event_id) && event_id != NULL_ID {
                visited.insert(event_id);
                if let Some(event) = self.dag_get(&event_id).await.unwrap() {
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
        let mut graph = HashMap::new();
        for iter_elem in self.main_dag.iter() {
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
    pub async fn fetch_successors_of(
        &self,
        tips: BTreeMap<u64, HashSet<Hash>>,
    ) -> Result<Vec<Event>> {
        debug!(
             target: "event_graph::fetch_successors_of",
             "fetching successors of {tips:?}"
        );

        let mut graph = HashMap::new();
        for iter_elem in self.main_dag.iter() {
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

async fn _send_request(peer: &Channel, missing: &Header) -> Result<Vec<Event>> {
    info!("in send_request first missing: {}", missing.id());
    let url = peer.address();
    debug!(target: "event_graph::dag_sync()","Requesting {:?} from {}...", missing, url);
    let ev_rep_sub = match peer.subscribe_msg::<EventRep>().await {
        Ok(v) => v,
        Err(e) => {
            error!(target: "event_graph::dag_sync()","[EVENTGRAPH] Sync: Couldn't subscribe EventRep for peer {}, skipping ({})",url, e);
            return Err(Error::Custom("Couldn't subscribe EventRep".to_string()))
        }
    };

    if let Err(e) = peer.send(&EventReq(vec![missing.id()])).await {
        error!(target: "event_graph::dag_sync()","[EVENTGRAPH] Sync: Failed communicating EventReq({:?}) to {}: {}",missing, url, e);
        return Err(Error::Custom("Failed communicating EventReq".to_string()))
    }

    let Ok(parent) = ev_rep_sub.receive_with_timeout(15).await else {
        error!(
            target: "event_graph::dag_sync()",
            "[EVENTGRAPH] Sync: Timeout waiting for parents {:?} from {}",
            missing, url,
        );
        return Err(().into())
    };

    Ok(parent.0.clone())
}

async fn request_header(peer: &Channel, comms_timeout: u64) -> Result<Vec<Header>> {
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

    if let Err(e) = peer.send(&HeaderReq {}).await {
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
    comms_timeout: u64,
) -> Result<Vec<Event>> {
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
            return Err(Error::EventNotFound("Couldn't subscribe EventRep".to_owned()));
        }
    };

    // let request_missing_events = missing_parents.clone().into_iter().collect();
    if let Err(e) = peer.send(&EventReq(headers.clone())).await {
        error!(
            target: "event_graph::dag_sync()",
            "[EVENTGRAPH] Sync: Failed communicating EventReq({:?}) to {}: {}",
            headers, url, e,
        );
        return Err(Error::EventNotFound("Failed communicating EventReq".to_owned()));
    }

    // Node waits for response
    let Ok(event) = ev_rep_sub.receive_with_timeout(comms_timeout).await else {
        error!(
            target: "event_graph::dag_sync()",
            "[EVENTGRAPH] Sync: Timeout waiting for parents {:?} from {}",
            headers, url,
        );
        return Err(Error::EventNotFound("Timeout waiting for parents".to_owned()));
    };

    Ok(event.0.clone())
}

fn peer_selection(peers: Vec<Arc<Channel>>) -> Arc<Channel> {
    peers.choose(&mut OsRng).unwrap().clone()
}

// async fn send_request(peer: &Channel, missing: &[Hash]) -> Result<Vec<Event>> {
//     info!("in send_request first missing: {}", missing[0]);
//     let url = peer.address();
//     debug!(target: "event_graph::dag_sync()","Requesting {:?} from {}...", missing, url);
//     let ev_rep_sub = match peer.subscribe_msg::<EventRep>().await {
//         Ok(v) => v,
//         Err(e) => {
//             error!(target: "event_graph::dag_sync()","[EVENTGRAPH] Sync: Couldn't subscribe EventRep for peer {}, skipping ({})",url, e);
//             return Err(Error::Custom("Couldn't subscribe EventRep".to_string()))
//         }
//     };

//     if let Err(e) = peer.send(&EventReq(missing.to_vec())).await {
//         error!(target: "event_graph::dag_sync()","[EVENTGRAPH] Sync: Failed communicating EventReq({:?}) to {}: {}",missing, url, e);
//         return Err(Error::Custom("Failed communicating EventReq".to_string()))
//     }

//     let Ok(parent) = ev_rep_sub.receive_with_timeout(15).await else {
//         error!(
//             target: "event_graph::dag_sync()",
//             "[EVENTGRAPH] Sync: Timeout waiting for parents {:?} from {}",
//             missing, url,
//         );
//         return Err(().into())
//     };

//     Ok(parent.0.clone())
// }

// // A function that sends requests to multiple peers concurrently
// async fn send_requests(peers: &[Arc<Channel>], missing: &[Hash]) -> Result<Vec<Vec<Event>>> {
//     info!("in send_requests first missing: {}", missing[0]);
//     let chunk_size = (missing.len() as f64 / peers.len() as f64).ceil() as usize;
//     let pairs = peers.iter().zip(missing.chunks(chunk_size)).collect::<Vec<_>>();

//     // For each peer, create a future that sends a request
//     let pair_stream = from_iter(pairs.iter());
//     let requests_stream = pair_stream.map(|(peer, missing)| send_request(peer, missing));

//     // Collect all the responses into a vector
//     let responses = requests_stream.collect::<Vec<_>>().await;

//     // Wait for all the futures to complete
//     future::try_join_all(responses).await
// }
