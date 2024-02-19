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

use std::{collections::HashSet, time::UNIX_EPOCH};

use darkfi_serial::{async_trait, deserialize_async, Encodable, SerialDecodable, SerialEncodable};
use sled_overlay::SledTreeOverlay;

use crate::Result;

use super::{
    util::next_rotation_timestamp, EventGraphPtr, EVENT_TIME_DRIFT, INITIAL_GENESIS, NULL_ID,
    N_EVENT_PARENTS,
};

/// Representation of an event in the Event Graph
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Event {
    /// Timestamp of the event
    pub(super) timestamp: u64,
    /// Content of the event
    pub(super) content: Vec<u8>,
    /// Parent nodes in the event DAG
    pub(super) parents: [blake3::Hash; N_EVENT_PARENTS],
    /// DAG layer index of the event
    pub(super) layer: u64,
}

impl Event {
    /// Create a new event with the given data and an [`EventGraph`] reference.
    /// The timestamp of the event will be the current time, and the parents
    /// will be `N_EVENT_PARENTS` from the current event graph unreferenced tips.
    /// The parents can also include NULL, but this should be handled by the rest
    /// of the codebase.
    pub async fn new(data: Vec<u8>, event_graph: &EventGraphPtr) -> Self {
        let (layer, parents) = event_graph.get_next_layer_with_parents().await;
        Self { timestamp: UNIX_EPOCH.elapsed().unwrap().as_secs(), content: data, parents, layer }
    }

    /// Hash the [`Event`] to retrieve its ID
    pub fn id(&self) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();
        self.timestamp.encode(&mut hasher).unwrap();
        self.content.encode(&mut hasher).unwrap();
        self.parents.encode(&mut hasher).unwrap();
        self.layer.encode(&mut hasher).unwrap();
        hasher.finalize()
    }

    /// Return a reference to the event's content
    pub fn content(&self) -> &[u8] {
        &self.content
    }

    /*
    /// Check if an [`Event`] is considered too old.
    fn is_too_old(&self) -> bool {
        self.timestamp < UNIX_EPOCH.elapsed().unwrap().as_secs() - ORPHAN_AGE_LIMIT
    }
    */

    /// Fully validate an event for the correct layout against provided
    /// DAG [`sled::Tree`] reference and enforce relevant age, assuming
    /// some possibility for a time drift. Optionally, provide an overlay
    /// to use that instead of actual referenced DAG.
    pub async fn validate(
        &self,
        dag: &sled::Tree,
        genesis_timestamp: u64,
        days_rotation: u64,
        overlay: Option<&SledTreeOverlay>,
    ) -> Result<bool> {
        // Let's not bother with empty events
        if self.content.is_empty() {
            return Ok(false)
        }

        // Check if the event timestamp is after genesis timestamp
        if self.timestamp < genesis_timestamp - EVENT_TIME_DRIFT {
            return Ok(false)
        }

        // If a rotation has been set, check if the event timestamp
        // is after the next genesis timestamp
        if days_rotation > 0 {
            let next_genesis_timestamp = next_rotation_timestamp(INITIAL_GENESIS, days_rotation);
            if self.timestamp > next_genesis_timestamp + EVENT_TIME_DRIFT {
                return Ok(false)
            }
        }

        // Validate the parents. We have to check that at least one parent
        // is not NULL, that the parents exist, that no two parents are the
        // same, and that the parent exists in previous layers, to prevent
        // recursive references(circles).
        let mut seen = HashSet::new();
        let self_id = self.id();

        for parent_id in self.parents.iter() {
            if parent_id == &NULL_ID {
                continue
            }

            if parent_id == &self_id {
                return Ok(false)
            }

            if seen.contains(parent_id) {
                return Ok(false)
            }

            let parent_bytes = if let Some(overlay) = overlay {
                overlay.get(parent_id.as_bytes())?
            } else {
                dag.get(parent_id.as_bytes())?
            };
            if parent_bytes.is_none() {
                return Ok(false)
            }

            let parent: Event = deserialize_async(&parent_bytes.unwrap()).await?;
            if self.layer <= parent.layer {
                return Ok(false)
            }

            seen.insert(parent_id);
        }

        Ok(!seen.is_empty())
    }

    /// Fully validate an event for the correct layout against provided
    /// [`EventGraph`] reference and enforce relevant age, assuming some
    /// possibility for a time drift.
    pub async fn dag_validate(&self, event_graph: &EventGraphPtr) -> Result<bool> {
        // Grab genesis timestamp
        let genesis_timestamp = event_graph.current_genesis.read().await.timestamp;

        // Perform validation
        self.validate(&event_graph.dag, genesis_timestamp, event_graph.days_rotation, None).await
    }

    /// Validate a new event for the correct layout and enforce relevant age,
    /// assuming some possibility for a time drift.
    /// Note: This validation does *NOT* check for recursive references(circles),
    /// and should be used as a first quick check.
    pub fn validate_new(&self) -> bool {
        // Let's not bother with empty events
        if self.content.is_empty() {
            return false
        }

        // Check if the event is too old or too new
        let now = UNIX_EPOCH.elapsed().unwrap().as_secs();
        let too_old = self.timestamp < now - EVENT_TIME_DRIFT;
        let too_new = self.timestamp > now + EVENT_TIME_DRIFT;

        if too_old || too_new {
            return false
        }

        // Validate the parents. We have to check that at least one parent
        // is not NULL and that no two parents are the same.
        let mut seen = HashSet::new();
        let self_id = self.id();

        for parent_id in self.parents.iter() {
            if parent_id == &NULL_ID {
                continue
            }

            if parent_id == &self_id {
                return false
            }

            if seen.contains(parent_id) {
                return false
            }

            seen.insert(parent_id);
        }

        !seen.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use smol::Executor;

    use crate::{
        event_graph::EventGraph,
        net::{P2p, Settings},
    };

    use super::*;

    async fn make_event_graph() -> Result<EventGraphPtr> {
        let ex = Arc::new(Executor::new());
        let p2p = P2p::new(Settings::default(), ex.clone()).await;
        let sled_db = sled::Config::new().temporary(true).open().unwrap();
        EventGraph::new(p2p, sled_db, "dag", 1, ex).await
    }

    #[test]
    fn event_is_valid() -> Result<()> {
        smol::block_on(async {
            // Generate a dummy event graph
            let event_graph = make_event_graph().await?;

            // Create a new valid event
            let valid_event = Event::new(vec![1u8], &event_graph).await;

            // Validate our test Event struct
            assert!(valid_event.dag_validate(&event_graph).await?);

            // Thanks for reading
            Ok(())
        })
    }

    #[test]
    fn invalid_events() -> Result<()> {
        smol::block_on(async {
            // Generate a dummy event graph
            let event_graph = make_event_graph().await?;

            // Create a new valid event
            let valid_event = Event::new(vec![1u8], &event_graph).await;

            let mut event_empty_content = valid_event.clone();
            event_empty_content.content = vec![];
            assert!(!event_empty_content.dag_validate(&event_graph).await?);

            let mut event_timestamp_too_old = valid_event.clone();
            event_timestamp_too_old.timestamp = 0;
            assert!(!event_timestamp_too_old.dag_validate(&event_graph).await?);

            let mut event_timestamp_too_new = valid_event.clone();
            event_timestamp_too_new.timestamp = u64::MAX;
            assert!(!event_timestamp_too_new.dag_validate(&event_graph).await?);

            let mut event_duplicated_parents = valid_event.clone();
            event_duplicated_parents.parents[1] = valid_event.parents[0];
            assert!(!event_duplicated_parents.dag_validate(&event_graph).await?);

            let mut event_null_parents = valid_event.clone();
            let all_null_parents = [NULL_ID, NULL_ID, NULL_ID, NULL_ID, NULL_ID];
            event_null_parents.parents = all_null_parents;
            assert!(!event_null_parents.dag_validate(&event_graph).await?);

            let mut event_same_layer_as_parents = valid_event.clone();
            event_same_layer_as_parents.layer = 0;
            assert!(!event_same_layer_as_parents.dag_validate(&event_graph).await?);

            // Thanks for reading
            Ok(())
        })
    }
}
