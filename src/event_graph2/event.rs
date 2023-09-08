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

use std::{collections::HashSet, time::UNIX_EPOCH};

use darkfi_serial::{async_trait, Encodable, SerialDecodable, SerialEncodable};

use super::{EventGraphPtr, EVENT_TIME_DRIFT, NULL_ID, N_EVENT_PARENTS};

/// Representation of an event in the Event Graph
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Event {
    /// Timestamp of the event
    pub(super) timestamp: u64,
    /// Content of the event
    pub(super) content: Vec<u8>,
    /// Parent nodes in the event DAG
    pub(super) parents: [blake3::Hash; N_EVENT_PARENTS],
}

impl Event {
    /// Create a new event with the given data and an [`EventGraph`] reference.
    /// The timestamp of the event will be the current time, and the parents
    /// will be `N_EVENT_PARENTS` from the current event graph unreferenced tips.
    /// The parents can also include NULL, but this should be handled by the rest
    /// of the codebase.
    pub async fn new(data: Vec<u8>, event_graph: EventGraphPtr) -> Self {
        Self {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_secs(),
            content: data,
            parents: event_graph.get_unreferenced_tips().await,
        }
    }

    /// Hash the [`Event`] to retrieve its ID
    pub fn id(&self) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();
        self.timestamp.encode(&mut hasher).unwrap();
        self.content.encode(&mut hasher).unwrap();
        self.parents.encode(&mut hasher).unwrap();
        hasher.finalize()
    }

    /*
    /// Check if an [`Event`] is considered too old.
    fn is_too_old(&self) -> bool {
        self.timestamp < UNIX_EPOCH.elapsed().unwrap().as_secs() - ORPHAN_AGE_LIMIT
    }
    */

    /// Validate a new event for the correct layout and enforce relevant age,
    /// assuming some possibility for a time drift.
    pub fn validate(&self) -> bool {
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
        // is not NULL, that the parent does not recursively reference the
        // event, and that no two parents are the same.
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
    use super::*;

    fn make_valid_event() -> Event {
        Event {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_secs(),
            content: vec![1u8],
            parents: [
                blake3::hash(b"1"),
                blake3::hash(b"2"),
                blake3::hash(b"3"),
                blake3::hash(b"4"),
                blake3::hash(b"5"),
            ],
        }
    }
    #[test]
    fn event_is_valid() {
        // Validate our test Event struct
        assert!(make_valid_event().validate());
    }

    #[test]
    fn invalid_events() {
        // TODO: Not checked:
        // - "the parent does not recursively reference the event"
        let e = make_valid_event();

        let mut event_empty_content = e.clone();
        event_empty_content.content = vec![];
        assert_eq!(event_empty_content.validate(), false);

        let mut event_timestamp_too_old = e.clone();
        event_timestamp_too_old.timestamp = 0;
        assert_eq!(event_timestamp_too_old.validate(), false);

        let mut event_timestamp_too_new = e.clone();
        event_timestamp_too_new.timestamp = u64::MAX;
        assert_eq!(event_timestamp_too_new.validate(), false);

        let mut event_duplicated_parents = e.clone();
        let duplicated_parents = [
            blake3::hash(b"1"),
            blake3::hash(b"1"),
            blake3::hash(b"3"),
            blake3::hash(b"4"),
            blake3::hash(b"5"),
        ];
        event_duplicated_parents.parents = duplicated_parents;
        assert_eq!(event_duplicated_parents.validate(), false);

        let mut event_null_parents = e.clone();
        let all_null_parents = [NULL_ID, NULL_ID, NULL_ID, NULL_ID, NULL_ID];
        event_null_parents.parents = all_null_parents;
        assert_eq!(event_null_parents.validate(), false);
    }
}
