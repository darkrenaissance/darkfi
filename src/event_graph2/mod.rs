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
    collections::{HashSet, VecDeque},
    sync::Arc,
};

use async_recursion::async_recursion;
use darkfi_serial::{deserialize_async, serialize_async};
use num_bigint::BigUint;
use smol::lock::RwLock;

use crate::{net::P2pPtr, Result};

/// An event graph event
pub mod event;
pub use event::Event;

/// P2P protocol implementation for the Event Graph
pub mod proto;

#[cfg(test)]
mod tests;

/// The number of parents an event is supposed to have.
const N_EVENT_PARENTS: usize = 5;
/// Allowed timestamp drift in seconds
const EVENT_TIME_DRIFT: u64 = 60;
// Allowed orphan age limit in seconds
//const ORPHAN_AGE_LIMIT: u64 = 60 * 5;
/// Null event ID
const NULL_ID: blake3::Hash = blake3::Hash::from_bytes([0x00; blake3::OUT_LEN]);

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
    /// The last event ID inserted into the DAG
    last_event: RwLock<blake3::Hash>,
    /// A `HashSet` containg event IDs and their 1-level parents.
    /// These come from the events we've sent out using `EventPut`.
    /// They are used with `EventReq` to decide if we should reply
    /// or not. Additionally it is also used when we broadcast the
    /// `TipRep` message telling peers about our unreferenced tips.
    broadcasted_ids: RwLock<HashSet<blake3::Hash>>,
}

impl EventGraph {
    /// Create a new [`EventGraph`] instance
    pub fn new(p2p: P2pPtr, sled_db: &sled::Db, dag_tree_name: &str) -> Result<EventGraphPtr> {
        let dag = sled_db.open_tree(dag_tree_name)?;
        let unreferenced_tips = RwLock::new(HashSet::new());
        let last_event = RwLock::new(NULL_ID);
        let broadcasted_ids = RwLock::new(HashSet::new());

        Ok(Arc::new(Self { p2p, dag, unreferenced_tips, last_event, broadcasted_ids }))
    }

    /// Sync the DAG from connected peers
    pub async fn dag_sync(&self) {
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
        todo!()
    }

    /// Prune the DAG
    pub async fn dag_prune(&self) {
        // The DAG should periodically be pruned. This can be a configurable
        // parameter. By pruning, we should deterministically replace the
        // genesis event (can use a deterministic timestamp) and drop everything
        // in the DAG, leaving just the new genesis event.
        // Do not use `chrono`.
        todo!()
    }

    /// Insert an event into the DAG.
    /// This will append the new event into the unreferenced tips set, and
    /// remove the event's parents from it. It will also append the event's
    /// level-1 parents to the `broadcasted_ids` set, so the P2P protocol
    /// knows that any requests for them are actually legitimate.
    /// TODO: The `broadcasted_ids` set should periodically be pruned, when
    /// some sensible time has passed after broadcasting the event.
    pub async fn dag_insert(&self, event: &Event) -> Result<blake3::Hash> {
        let event_id = event.id();
        let s_event = serialize_async(event).await;

        let mut unreferenced_tips = self.unreferenced_tips.write().await;
        let mut bcast_ids = self.broadcasted_ids.write().await;
        for parent_id in event.parents.iter() {
            if parent_id != &NULL_ID {
                unreferenced_tips.remove(parent_id);
                bcast_ids.insert(*parent_id);
            }
        }
        unreferenced_tips.insert(event_id);
        drop(unreferenced_tips);
        drop(bcast_ids);

        self.dag.insert(event_id.as_bytes(), s_event).unwrap();
        *self.last_event.write().await = event_id;

        Ok(event_id)
    }

    /// Get the current set of unreferenced tips in the DAG.
    async fn get_unreferenced_tips(&self) -> [blake3::Hash; N_EVENT_PARENTS] {
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
                if event.timestamp.0 < existing_event.timestamp.0 {
                    pos = idx;
                } else if event.timestamp.0 == existing_event.timestamp.0 {
                    // In case of a tie-breaker, use the event ID
                    let a = BigUint::from_bytes_be(event_id.as_bytes());
                    let b = BigUint::from_bytes_be(existing_id.as_bytes());
                    if a < b {
                        pos = idx;
                    }
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
