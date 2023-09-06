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
    /// or not.
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

    /// Insert an event into the DAG
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

    /// Get a set of unreferenced tips used to produce a new [`Event`]
    async fn get_unreferenced_tips(&self) -> [blake3::Hash; N_EVENT_PARENTS] {
        let mut tips = [NULL_ID; N_EVENT_PARENTS];
        let unreferenced_tips = self.unreferenced_tips.read().await;

        for (i, tip) in unreferenced_tips.iter().enumerate() {
            if i == N_EVENT_PARENTS - 1 {
                break
            }

            tips[i] = *tip;
        }

        assert!(tips.iter().any(|x| x != &NULL_ID));
        tips
    }

    /// Perform a topological sort of the DAG.
    pub async fn order_events(&self) -> Vec<blake3::Hash> {
        let mut ordered_events = VecDeque::new();
        let mut visited = HashSet::new();

        for tip in self.get_unreferenced_tips().await {
            if !visited.contains(&tip) && tip != NULL_ID {
                let tip = self.dag.get(tip.as_bytes()).unwrap().unwrap();
                let tip = deserialize_async(&tip).await.unwrap();
                self.dfs_topological_sort(tip, &mut visited, &mut ordered_events).await;
            }
        }

        ordered_events.make_contiguous().to_vec()
    }

    /// <https://en.wikipedia.org/wiki/Depth-first_search>
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

        // Once all the parents are visited, add the current event
        // to the start of the list
        ordered_events.push_front(event_id);
    }
}
