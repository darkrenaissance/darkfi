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

//! Timestamp arithmetic, genesis generation, and replay logging.

use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    time::UNIX_EPOCH,
};

use darkfi_serial::{deserialize, deserialize_async, serialize};
use sled_overlay::sled;
use tinyjson::JsonValue;

use super::{
    event::{Event, Header},
    EventGraphConfig, NULL_ID, N_EVENT_PARENTS,
};
use crate::{
    util::{encoding::base64, file::load_file},
    Error, Result,
};

#[cfg(feature = "rpc")]
use crate::rpc::{
    jsonrpc::{ErrorCode, JsonError, JsonResponse, JsonResult},
    util::json_map,
};

/// Milliseconds in one hour.
pub(super) const HOUR_MS: u64 = 3_600_000;

/// Current UNIX timestamp in milliseconds.
pub(super) fn unix_timestamp_millis() -> Result<u64> {
    let elapsed = UNIX_EPOCH
        .elapsed()
        .map_err(|_| Error::Custom("system clock is before UNIX epoch".into()))?;
    u64::try_from(elapsed.as_millis())
        .map_err(|_| Error::Custom("system clock milliseconds exceed u64".into()))
}

/// Timestamp (millis) for the start of the hour `hours` offsets from now.
pub(super) fn next_hour_timestamp(hours: i64) -> Result<u64> {
    let now = unix_timestamp_millis()?;
    let base = (now / HOUR_MS) * HOUR_MS;
    let offset = hours.unsigned_abs().saturating_mul(HOUR_MS);

    if hours.is_negative() {
        Ok(base.saturating_sub(offset))
    } else {
        Ok(base.saturating_add(offset))
    }
}

/// Whole hours elapsed since `ts`.
pub(super) fn hours_since(ts: u64) -> Result<u64> {
    let now = unix_timestamp_millis()?;
    Ok(now.saturating_sub(ts) / HOUR_MS)
}

/// Timestamp of the next DAG rotation.
pub fn next_rotation_timestamp(starting_timestamp: u64, rotation_period: u64) -> Result<u64> {
    if rotation_period == 0 {
        return Err(Error::Custom("event graph rotation period cannot be 0".into()))
    }

    let rotation_ms = rotation_period.checked_mul(HOUR_MS).ok_or_else(|| {
        Error::Custom("event graph rotation period overflows milliseconds".into())
    })?;
    let now = unix_timestamp_millis()?;

    if now < starting_timestamp {
        return Ok(starting_timestamp)
    }

    let elapsed = now.saturating_sub(starting_timestamp);
    let periods = elapsed
        .checked_div(rotation_ms)
        .and_then(|p| p.checked_add(1))
        .ok_or_else(|| Error::Custom("event graph rotation calculation overflowed".into()))?;
    let offset = periods
        .checked_mul(rotation_ms)
        .ok_or_else(|| Error::Custom("event graph rotation offset overflowed".into()))?;

    starting_timestamp
        .checked_add(offset)
        .ok_or_else(|| Error::Custom("event graph next rotation timestamp overflowed".into()))
}

/// Milliseconds remaining until `next_rotation`.
pub fn millis_until_next_rotation(next_rotation: u64) -> Result<u64> {
    let now = unix_timestamp_millis()?;
    next_rotation
        .checked_sub(now)
        .ok_or_else(|| Error::Custom("event graph next rotation is in the past".into()))
}

/// Generate the deterministic genesis event for the current rotation
/// period, using the caller-provided [`EventGraphConfig`].
///
/// * `hours_rotation == 0` -> timestamp is `initial_genesis`.
/// * `hours_rotation > 0`  -> timestamp is the most recent
///   multiple-of-N boundary since `initial_genesis`.
pub fn generate_genesis(config: &EventGraphConfig) -> Result<Event> {
    let timestamp = if config.hours_rotation == 0 {
        config.initial_genesis
    } else {
        let passed = hours_since(config.initial_genesis)?;
        let rotations = passed
            .checked_div(config.hours_rotation)
            .ok_or_else(|| Error::Custom("event graph rotation period cannot be 0".into()))?;
        let offset_hours = rotations.saturating_mul(config.hours_rotation);
        let offset_ms = offset_hours.saturating_mul(HOUR_MS);
        config.initial_genesis.saturating_add(offset_ms)
    };
    let content_hash = blake3::hash(&config.genesis_contents);
    let header = Header { timestamp, parents: [NULL_ID; N_EVENT_PARENTS], layer: 0, content_hash };
    Ok(Event { header, content: config.genesis_contents.clone() })
}

/// Append a replayer log entry for DAG state recreation.
pub(super) fn replayer_log(datastore: &Path, cmd: String, value: Vec<u8>) -> Result<()> {
    fs::create_dir_all(datastore)?;
    let p = datastore.join("replayer.log");
    if !p.exists() {
        File::create(&p)?;
    }
    let mut f = OpenOptions::new().append(true).open(&p)?;
    writeln!(f, "{cmd} {}", base64::encode(&value))?;
    Ok(())
}

#[cfg(feature = "rpc")]
pub async fn recreate_from_replayer_log(datastore: &Path) -> JsonResult {
    let log_path = datastore.join("replayer.log");
    if !log_path.exists() {
        return JsonResult::Error(JsonError::new(
            ErrorCode::ParseError,
            Some("Log not found".into()),
            1,
        ))
    }
    let replay_error =
        |e: String| JsonResult::Error(JsonError::new(ErrorCode::ParseError, Some(e), 1));
    let reader = match load_file(&log_path) {
        Ok(reader) => reader,
        Err(e) => return replay_error(e.to_string()),
    };
    let sled_db = match sled::open(datastore.join("replayed_db")) {
        Ok(db) => db,
        Err(e) => return replay_error(e.to_string()),
    };
    let dag = match sled_db.open_tree("replayer") {
        Ok(tree) => tree,
        Err(e) => return replay_error(e.to_string()),
    };
    for line in reader.lines() {
        let parts = line.split(' ').collect::<Vec<&str>>();
        if parts.first() == Some(&"insert") {
            let Some(encoded) = parts.get(1) else {
                return replay_error("malformed event graph replay insert entry".into())
            };
            let Some(bytes) = base64::decode(encoded) else {
                return replay_error("invalid base64 in event graph replay log".into())
            };
            let v: Event = match deserialize(&bytes) {
                Ok(event) => event,
                Err(e) => return replay_error(e.to_string()),
            };
            if let Err(e) = dag.insert(v.header.id().as_bytes(), serialize(&v)) {
                return replay_error(e.to_string())
            }
        }
    }
    let mut graph = HashMap::new();
    for item in dag.iter() {
        let (id, val) = match item {
            Ok(item) => item,
            Err(e) => return replay_error(e.to_string()),
        };
        let id = match <[u8; 32]>::try_from(&id as &[u8]) {
            Ok(bytes) => blake3::Hash::from_bytes(bytes),
            Err(e) => return replay_error(e.to_string()),
        };
        let event = match deserialize_async::<Event>(&val).await {
            Ok(event) => event,
            Err(e) => return replay_error(e.to_string()),
        };
        graph.insert(id, event);
    }
    let json_graph = graph.into_iter().map(|(k, v)| (k.to_string(), JsonValue::from(v))).collect();
    let values = json_map([("dag", JsonValue::Object(json_graph))]);
    JsonResponse::new(JsonValue::Object(HashMap::from([("eventgraph_info".into(), values)])), 1)
        .into()
}

/// Which DAG an event came from. Used by [`event_to_gource`] to
/// pick the appropriate path prefix when formatting visualization
/// output.
#[derive(Copy, Clone, Debug)]
pub enum DagKind {
    /// A rotating-DAG event (regular IRC traffic, etc.)
    Rotating,
    /// A static-DAG event (RLN registration or slash)
    Static,
}

impl DagKind {
    fn path_prefix(self) -> &'static str {
        match self {
            DagKind::Rotating => "rotating",
            DagKind::Static => "static",
        }
    }
}

/// Format an [`Event`] as a single Gource custom-log line.
///
/// Output format (Gource custom log, pipe-delimited):
///
/// ```text
///     <unix-seconds>|<username>|A|/<dag-kind>/<layer>/<event-id-prefix>
/// ```
pub fn event_to_gource(ev: &Event, kind: DagKind) -> String {
    let unix_secs = ev.header.timestamp / 1_000;

    // First non-NULL parent -> 8-char hex prefix; otherwise "genesis".
    let username = ev
        .header
        .parents
        .iter()
        .find(|p| **p != NULL_ID)
        .map(|p| {
            let hex = p.to_hex();
            hex[..8.min(hex.len())].to_string()
        })
        .unwrap_or_else(|| "genesis".to_string());

    let id_hex = ev.id().to_hex();
    let id_prefix = &id_hex[..16.min(id_hex.len())];

    format!(
        "{}|{}|A|/{}/{:06}/{}",
        unix_secs,
        username,
        kind.path_prefix(),
        ev.header.layer,
        id_prefix,
    )
}
