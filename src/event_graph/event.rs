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

use std::{cmp::Ordering, collections::HashSet, time::UNIX_EPOCH};

use darkfi_serial::{async_trait, deserialize_async, Encodable, SerialDecodable, SerialEncodable};
use sled_overlay::{sled, SledTreeOverlay};

use super::{util::HOUR, EventGraph, EventGraphConfig, EVENT_TIME_DRIFT, NULL_ID, N_EVENT_PARENTS};
use crate::Result;

/// The fixed-size structural metadata of an event.
///
/// Headers are lightweight and encode the full DAG topology without
/// carrying the variable-length content. The content is committed
/// to via `content_hash`, so peers can verify the integrity of an
/// event body against the header that announced it.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Header {
    /// UNIX timestamp of the event in milliseconds.
    pub timestamp: u64,
    /// Parent references. Unused slots are [`NULL_ID`].
    pub parents: [blake3::Hash; N_EVENT_PARENTS],
    /// Monotonically increasing layer index.
    pub layer: u64,
    /// blake3 hash of the event's content payload
    pub content_hash: blake3::Hash,
}

impl Header {
    pub async fn new(content: &[u8], eg: &EventGraph) -> Self {
        let dag_ts = eg.current_genesis.read().await.header.timestamp;
        let (layer, parents) = eg.get_next_layer_with_parents(&dag_ts).await;
        Self {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64,
            parents,
            layer,
            content_hash: blake3::hash(content),
        }
    }

    pub async fn new_static(content: &[u8], eg: &EventGraph) -> Self {
        let (layer, parents) = eg.get_next_layer_with_parents_static().await;
        Self {
            timestamp: UNIX_EPOCH.elapsed().unwrap().as_millis() as u64,
            parents,
            layer,
            content_hash: blake3::hash(content),
        }
    }

    pub async fn with_timestamp(timestamp: u64, content: &[u8], eg: &EventGraph) -> Self {
        let dag_ts = eg.current_genesis.read().await.header.timestamp;
        let (layer, parents) = eg.get_next_layer_with_parents(&dag_ts).await;
        Self { timestamp, parents, layer, content_hash: blake3::hash(content) }
    }

    /// Blake3 hash of `(timestamp, parents, layer, content_hash)`.
    pub fn id(&self) -> blake3::Hash {
        let mut h = blake3::Hasher::new();
        self.timestamp.encode(&mut h).unwrap();
        self.parents.encode(&mut h).unwrap();
        self.layer.encode(&mut h).unwrap();
        h.update(self.content_hash.as_bytes());
        h.finalize()
    }

    /// Full structural validation against a header DAG.
    ///
    /// `dag_genesis` is the timestamp/name of the target rotating DAG slot.
    pub async fn validate(
        &self,
        header_dag: &sled::Tree,
        config: &EventGraphConfig,
        dag_genesis: u64,
        overlay: Option<&SledTreeOverlay>,
    ) -> Result<bool> {
        if !self.timestamp_fits_slot(config, dag_genesis) {
            return Ok(false)
        }

        let mut seen = HashSet::new();
        let mut max_parent_layer = None;
        let self_id = self.id();
        for pid in self.parents.iter() {
            if pid == &NULL_ID {
                continue
            }

            if pid == &self_id || seen.contains(pid) {
                return Ok(false)
            }

            let bytes = if let Some(ov) = overlay {
                ov.get(pid.as_bytes())?
            } else {
                header_dag.get(pid.as_bytes())?
            };

            let Some(bytes) = bytes else { return Ok(false) };
            let parent: Header = deserialize_async(&bytes).await?;
            max_parent_layer =
                Some(max_parent_layer.map_or(parent.layer, |m: u64| m.max(parent.layer)));
            seen.insert(pid);
        }

        let Some(max_parent_layer) = max_parent_layer else { return Ok(false) };
        let Some(expected_layer) = max_parent_layer.checked_add(1) else { return Ok(false) };

        Ok(self.layer == expected_layer)
    }

    /// Check whether this header timestamp belongs to the target DAG slot.
    fn timestamp_fits_slot(&self, config: &EventGraphConfig, dag_genesis: u64) -> bool {
        if self.timestamp < dag_genesis.saturating_sub(EVENT_TIME_DRIFT) {
            return false
        }

        if config.hours_rotation == 0 {
            let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
            return self.timestamp <= now.saturating_add(EVENT_TIME_DRIFT)
        }

        let Some(rotation_ms) = config.hours_rotation.checked_mul(HOUR as u64) else {
            return false
        };
        let Some(next_slot) = dag_genesis.checked_add(rotation_ms) else { return false };
        let Some(upper_bound) = next_slot.checked_add(EVENT_TIME_DRIFT) else { return false };

        self.timestamp < upper_bound
    }
}

/// A complete event: [`Header`] + application-defined content.
#[derive(Debug, Clone, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Event {
    pub header: Header,
    /// Application payload. Must not be empty for non-genesis events.
    pub content: Vec<u8>,
}

impl Event {
    pub async fn new(data: Vec<u8>, eg: &EventGraph) -> Self {
        let header = Header::new(&data, eg).await;
        Self { header, content: data }
    }

    pub async fn new_static(data: Vec<u8>, eg: &EventGraph) -> Self {
        let header = Header::new_static(&data, eg).await;
        Self { header, content: data }
    }

    pub fn id(&self) -> blake3::Hash {
        self.header.id()
    }

    pub async fn with_timestamp(ts: u64, data: Vec<u8>, eg: &EventGraph) -> Self {
        let header = Header::with_timestamp(ts, &data, eg).await;
        Self { header, content: data }
    }

    pub fn content(&self) -> &[u8] {
        &self.content
    }

    /// Check that the content matches the hash committed to in the header.
    pub fn content_matches_header(&self) -> bool {
        blake3::hash(&self.content) == self.header.content_hash
    }

    /// Validate for insertion into a DAG.
    ///
    /// `dag_genesis` is the timestamp/name of the target rotating DAG slot.
    pub async fn dag_validate(
        &self,
        hdr_dag: &sled::Tree,
        config: &EventGraphConfig,
        dag_genesis: u64,
    ) -> Result<bool> {
        if self.content.is_empty() {
            return Ok(false)
        }

        if !self.content_matches_header() {
            return Ok(false)
        }

        self.header.validate(hdr_dag, config, dag_genesis, None).await
    }

    /// Quick validation (no DAG lookup).
    pub fn validate_new(&self) -> bool {
        if !self.validate_new_common() {
            return false
        }

        let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;

        if self.header.timestamp < now - EVENT_TIME_DRIFT ||
            self.header.timestamp > now + EVENT_TIME_DRIFT
        {
            return false
        }

        true
    }

    /// Quick validation for static-DAG events.
    ///
    /// Static-DAG events (RLN registrations and slashes) are
    /// persistent across rotation windows by design - they form
    /// the consensus identity tree and a node syncing for the
    /// first time may legitimately receive registrations that are
    /// hours, days, or longer old. Rejecting them on a 60-second
    /// time-drift window (as `validate_new` does for rotating
    /// events, where freshness IS part of the threat model)
    /// would prevent any late-joining node from ever syncing
    /// historical RLN state.
    ///
    /// This method runs the same structural checks as
    /// `validate_new` (non-empty content, content matches header,
    /// well-formed parent set) but omits the drift-window check.
    /// RLN proof verification of the static event itself happens
    /// separately in `EventGraph::rln_verify_static_event`.
    pub fn validate_new_static(&self) -> bool {
        self.validate_new_common()
    }

    /// Shared validation between `validate_new` and
    /// `validate_new_static`. Returns false if the event is
    /// structurally malformed in any time-independent way.
    fn validate_new_common(&self) -> bool {
        if self.content.is_empty() {
            return false
        }

        if !self.content_matches_header() {
            return false
        }

        let mut seen = HashSet::new();
        let sid = self.header.id();
        for pid in self.header.parents.iter() {
            if pid == &NULL_ID {
                continue
            }
            if pid == &sid || seen.contains(pid) {
                return false
            }
            seen.insert(pid);
        }

        !seen.is_empty()
    }
}

/// Chronological comparator with deterministic hash tie-breaking.
pub fn display_order(a: &Event, b: &Event) -> Ordering {
    a.header
        .timestamp
        .cmp(&b.header.timestamp)
        .then_with(|| a.id().as_bytes().cmp(b.id().as_bytes()))
}
