/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

use darkfi::{
    event_graph::{
        util::{generate_genesis, millis_until_next_rotation, next_rotation_timestamp},
        Event, GENESIS_CONTENTS, INITIAL_GENESIS, NULL_ID, N_EVENT_PARENTS,
    },
    system::{msleep, Publisher, PublisherPtr, StoppableTask, StoppableTaskPtr},
    Error, Result,
};
use darkfi_serial::{
    async_trait, deserialize_async, serialize_async, SerialDecodable, SerialEncodable,
};
use sled_overlay::{sled, SledTreeOverlay};
use smol::{
    lock::{OnceCell, RwLock},
    Executor,
};
use std::{
    collections::{BTreeMap, HashSet},
    sync::Arc,
};
use tracing::{debug, error, info};

pub const PROTOCOL_VERSION: u32 = 1;

/// Atomic pointer to an [`EventGraph`] instance.
pub type LocalEventGraphPtr = Arc<LocalEventGraph>;

pub struct LocalEventGraph {
    /// Sled tree containing the DAG
    pub dag: sled::Tree,
    /// The set of unreferenced DAG tips
    pub unreferenced_tips: RwLock<BTreeMap<u64, HashSet<blake3::Hash>>>,
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
    pub days_rotation: u64,
    /// Flag signalling DAG has finished initial sync
    pub synced: RwLock<bool>,
    /// Enable graph debugging
    pub deg_enabled: RwLock<bool>,
}

impl LocalEventGraph {
    pub async fn new(
        sled_db: sled::Db,
        dag_tree_name: &str,
        days_rotation: u64,
        ex: Arc<Executor<'_>>,
    ) -> Result<LocalEventGraphPtr> {
        let dag = sled_db.open_tree(dag_tree_name)?;
        let unreferenced_tips = RwLock::new(BTreeMap::new());
        let broadcasted_ids = RwLock::new(HashSet::new());
        let event_pub = Publisher::new();

        // Create the current genesis event based on the `days_rotation`
        let current_genesis = generate_genesis(days_rotation);
        let self_ = Arc::new(Self {
            dag: dag.clone(),
            unreferenced_tips,
            broadcasted_ids,
            prune_task: OnceCell::new(),
            event_pub,
            current_genesis: RwLock::new(current_genesis.clone()),
            days_rotation,
            synced: RwLock::new(false),
            deg_enabled: RwLock::new(false),
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
            let prune_task = StoppableTask::new();
            let _ = self_.prune_task.set(prune_task.clone()).await;

            prune_task.clone().start(
                self_.clone().dag_prune_task(days_rotation),
                |_| async move {
                    info!(target: "event_graph::_handle_stop()", "[EVENTGRAPH] Prune task stopped, flushing sled")
                },
                Error::DetachedTaskStopped,
                ex.clone(),
            );
        }

        Ok(self_)
    }

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
            let s = millis_until_next_rotation(next_rotation);

            debug!(target: "event_graph::dag_prune_task()", "Sleeping {}s until next DAG prune", s);
            msleep(s).await;
            debug!(target: "event_graph::dag_prune_task()", "Rotation period reached");

            // Trigger DAG prune
            self.dag_prune(current_genesis).await?;
        }
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
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VersionMessage {
    pub protocol_version: u32,
}

impl VersionMessage {
    pub fn new() -> Self {
        Self { protocol_version: PROTOCOL_VERSION }
    }
}

impl Default for VersionMessage {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FetchEventsMessage {
    pub unref_tips: BTreeMap<u64, HashSet<blake3::Hash>>,
}

impl FetchEventsMessage {
    pub fn new(unref_tips: BTreeMap<u64, HashSet<blake3::Hash>>) -> Self {
        Self { unref_tips }
    }
}

pub const MSG_EVENT: u8 = 1;
pub const MSG_FETCHEVENTS: u8 = 2;
pub const MSG_SENDEVENT: u8 = 3;
