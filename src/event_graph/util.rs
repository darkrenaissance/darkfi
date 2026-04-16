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

use crate::{
    event_graph::{Event, EventGraphConfig, NULL_ID, N_EVENT_PARENTS},
    util::{encoding::base64, file::load_file},
    Result,
};

#[cfg(feature = "rpc")]
use crate::rpc::{
    jsonrpc::{ErrorCode, JsonError, JsonResponse, JsonResult},
    util::json_map,
};

use super::event::Header;

/// Milliseconds in one hour.
pub(super) const HOUR: i64 = 3_600_000;

/// Timestamp (millis) for the start of the hour `hours` offsets from now.
pub(super) fn next_hour_timestamp(hours: i64) -> u64 {
    let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as i64;
    ((now / HOUR) * HOUR + HOUR * hours) as u64
}

/// Whole hours elapsed since `ts`.
pub(super) fn hours_since(ts: u64) -> u64 {
    let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
    (now - ts) / HOUR as u64
}

/// Timestamp of the next DAG rotation.
///
/// # Panics
///
/// Panics if `rotation_period` is zero.
pub fn next_rotation_timestamp(starting_timestamp: u64, rotation_period: u64) -> u64 {
    if rotation_period == 0 {
        panic!("Rotation period cannot be 0");
    }
    let passed = hours_since(starting_timestamp);
    let rotations = passed.div_ceil(rotation_period);
    let until: i64 = (rotations * rotation_period - passed).try_into().unwrap();
    if until == 0 {
        next_hour_timestamp(1)
    } else {
        next_hour_timestamp(until)
    }
}

/// Milliseconds remaining until `next_rotation`.
///
/// # Panics
///
/// Panics if `next_rotation` is in the past.
pub fn millis_until_next_rotation(next_rotation: u64) -> u64 {
    let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
    assert!(next_rotation >= now, "Next rotation is in the past");
    next_rotation - now
}

/// Generate the deterministic genesis event for the current rotation
/// period, using the caller-provided [`EventGraphConfig`].
///
/// * `hours_rotation == 0` → timestamp is `initial_genesis`.
/// * `hours_rotation > 0`  → timestamp is the most recent
///   multiple-of-N boundary since `initial_genesis`.
pub fn generate_genesis(config: &EventGraphConfig) -> Event {
    let timestamp = if config.hours_rotation == 0 {
        config.initial_genesis
    } else {
        let passed = hours_since(config.initial_genesis);
        let rotations = passed / config.hours_rotation;
        config.initial_genesis + (rotations * config.hours_rotation * HOUR as u64)
    };
    let content_hash = blake3::hash(&config.genesis_contents);
    let header = Header { timestamp, parents: [NULL_ID; N_EVENT_PARENTS], layer: 0, content_hash };
    Event { header, content: config.genesis_contents.clone() }
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
    let reader = load_file(&log_path).unwrap();
    let sled_db = sled::open(datastore.join("replayed_db")).unwrap();
    let dag = sled_db.open_tree("replayer").unwrap();
    for line in reader.lines() {
        let parts = line.split(' ').collect::<Vec<&str>>();
        if parts[0] == "insert" {
            let v: Event = deserialize(&base64::decode(parts[1]).unwrap()).unwrap();
            dag.insert(v.header.id().as_bytes(), serialize(&v)).unwrap();
        }
    }
    let mut graph = HashMap::new();
    for item in dag.iter() {
        let (id, val) = item.unwrap();
        let id = blake3::Hash::from_bytes((&id as &[u8]).try_into().unwrap());
        graph.insert(id, deserialize_async::<Event>(&val).await.unwrap());
    }
    let json_graph = graph.into_iter().map(|(k, v)| (k.to_string(), JsonValue::from(v))).collect();
    let values = json_map([("dag", JsonValue::Object(json_graph))]);
    JsonResponse::new(JsonValue::Object(HashMap::from([("eventgraph_info".into(), values)])), 1)
        .into()
}
