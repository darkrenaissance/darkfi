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

use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    time::UNIX_EPOCH,
};

use crate::{
    event_graph::{Event, GENESIS_CONTENTS, INITIAL_GENESIS, NULL_ID, N_EVENT_PARENTS},
    util::encoding::base64,
    Result,
};

#[cfg(feature = "rpc")]
use {
    crate::{
        rpc::{
            jsonrpc::{ErrorCode, JsonError, JsonResponse, JsonResult},
            util::json_map,
        },
        util::file::load_file,
    },
    darkfi_serial::{deserialize, deserialize_async, serialize},
    sled_overlay::sled,
    std::collections::HashMap,
    tinyjson::JsonValue,
    tracing::error,
};

/// MilliSeconds in a day
pub(super) const DAY: i64 = 86_400_000;

/// Calculate the midnight timestamp given a number of days.
/// If `days` is 0, calculate the midnight timestamp of today.
pub(super) fn midnight_timestamp(days: i64) -> u64 {
    // Get current time
    let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as i64;

    // Find the timestamp for the midnight of the current day
    let cur_midnight = (now / DAY) * DAY;

    // Adjust for days_from_now
    (cur_midnight + (DAY * days)) as u64
}

/// Calculate the number of days since a given midnight timestamp.
pub(super) fn days_since(midnight_ts: u64) -> u64 {
    // Get current time
    let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;

    // Calculate the difference between the current timestamp
    // and the given midnight timestamp
    let elapsed_seconds = now - midnight_ts;

    // Convert the elapsed seconds into days
    elapsed_seconds / DAY as u64
}

/// Calculate the timestamp of the next DAG rotation.
pub fn next_rotation_timestamp(starting_timestamp: u64, rotation_period: u64) -> u64 {
    // Prevent division by 0
    if rotation_period == 0 {
        panic!("Rotation period cannot be 0");
    }
    // Calculate the number of days since the given starting point
    let days_passed = days_since(starting_timestamp);

    // Find out how many rotation periods have occurred since
    // the starting point.
    // Note: when rotation_period = 1, rotations_since_start = days_passed
    let rotations_since_start = days_passed.div_ceil(rotation_period);

    // Find out the number of days until the next rotation. Panic if result is beyond the range
    // of i64.
    let days_until_next_rotation: i64 =
        (rotations_since_start * rotation_period - days_passed).try_into().unwrap();

    // Get the timestamp for the next rotation
    if days_until_next_rotation == 0 {
        // If there are 0 days until the next rotation, we want
        // to rotate tomorrow, at midnight. This is a special case.
        return midnight_timestamp(1)
    }
    midnight_timestamp(days_until_next_rotation)
}

/// Calculate the time in milliseconds until the next_rotation, given
/// as a timestamp.
/// `next_rotation` here represents a timestamp in UNIX epoch format.
pub fn millis_until_next_rotation(next_rotation: u64) -> u64 {
    // Store `now` in a variable in order to avoid a TOCTOU error.
    // There may be a drift of one second between this panic check and
    // the return value if we get unlucky.
    let now = UNIX_EPOCH.elapsed().unwrap().as_millis() as u64;
    if next_rotation < now {
        panic!("Next rotation timestamp is in the past");
    }
    next_rotation - now
}

/// Generate a deterministic genesis event corresponding to the DAG's configuration.
pub fn generate_genesis(days_rotation: u64) -> Event {
    // Days rotation is u64 except zero
    let timestamp = if days_rotation == 0 {
        INITIAL_GENESIS
    } else {
        // First check how many days passed since initial genesis.
        let days_passed = days_since(INITIAL_GENESIS);

        // Calculate the number of days_rotation intervals since INITIAL_GENESIS
        let rotations_since_genesis = days_passed / days_rotation;

        // Calculate the timestamp of the most recent event
        INITIAL_GENESIS + (rotations_since_genesis * days_rotation * DAY as u64)
    };
    Event {
        timestamp,
        content: GENESIS_CONTENTS.to_vec(),
        parents: [NULL_ID; N_EVENT_PARENTS],
        layer: 0,
    }
}

pub(super) fn replayer_log(datastore: &Path, cmd: String, value: Vec<u8>) -> Result<()> {
    fs::create_dir_all(datastore)?;
    let datastore = datastore.join("replayer.log");
    if !datastore.exists() {
        File::create(&datastore)?;
    };

    let mut file = OpenOptions::new().append(true).open(&datastore)?;
    let v = base64::encode(&value);
    let f = format!("{cmd} {v}");
    writeln!(file, "{f}")?;

    Ok(())
}

#[cfg(feature = "rpc")]
pub async fn recreate_from_replayer_log(datastore: &Path) -> JsonResult {
    let log_path = datastore.join("replayer.log");
    if !log_path.exists() {
        error!("Error loading replayed log");
        return JsonResult::Error(JsonError::new(
            ErrorCode::ParseError,
            Some("Error loading replayed log".to_string()),
            1,
        ))
    };

    let reader = load_file(&log_path).unwrap();

    let db_datastore = datastore.join("replayed_db");

    let sled_db = sled::open(db_datastore).unwrap();
    let dag = sled_db.open_tree("replayer").unwrap();

    for line in reader.lines() {
        let line = line.split(' ').collect::<Vec<&str>>();
        if line[0] == "insert" {
            let v = base64::decode(line[1]).unwrap();
            let v: Event = deserialize(&v).unwrap();
            let v_se = serialize(&v);
            dag.insert(v.id().as_bytes(), v_se).unwrap();
        }
    }

    let mut graph = HashMap::new();
    for iter_elem in dag.iter() {
        let (id, val) = iter_elem.unwrap();
        let id = blake3::Hash::from_bytes((&id as &[u8]).try_into().unwrap());
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

    JsonResponse::new(result, 1).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_days_since() {
        let five_days_ago = midnight_timestamp(-5);
        assert_eq!(days_since(five_days_ago), 5);

        let today = midnight_timestamp(0);
        assert_eq!(days_since(today), 0);
    }

    #[test]
    fn test_next_rotation_timestamp() {
        let starting_point = midnight_timestamp(-10);
        let rotation_period = 7;

        // The first rotation since the starting point would be 3 days ago.
        // So the next rotation should be 4 days from now.
        let expected = midnight_timestamp(4);
        assert_eq!(next_rotation_timestamp(starting_point, rotation_period), expected);

        // When starting from today with a rotation period of 1 (day),
        // we should get tomorrow's timestamp.
        // This is a special case.
        let midnight_today: u64 = midnight_timestamp(0);
        let midnight_tomorrow = midnight_today + 86_400_000u64; // add a day
        assert_eq!(midnight_tomorrow, next_rotation_timestamp(midnight_today, 1));
    }

    #[test]
    #[should_panic]
    fn test_next_rotation_timestamp_panics_on_overflow() {
        next_rotation_timestamp(0, u64::MAX);
    }

    #[test]
    #[should_panic]
    fn test_next_rotation_timestamp_panics_on_division_by_zero() {
        next_rotation_timestamp(0, 0);
    }

    #[test]
    fn test_millis_until_next_rotation_is_within_rotation_interval() {
        let days_rotation = 1u64;
        // The amount of time in seconds between rotations.
        let rotation_interval = days_rotation * 86_400_000u64;
        let next_rotation_timestamp = next_rotation_timestamp(INITIAL_GENESIS, days_rotation);
        let s = millis_until_next_rotation(next_rotation_timestamp);
        assert!(s < rotation_interval);
    }
}
