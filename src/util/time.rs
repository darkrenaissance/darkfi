/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use std::time::UNIX_EPOCH;

use chrono::{NaiveDateTime, Utc};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use serde::{Deserialize, Serialize};

use crate::Result;

/// Wrapper struct to represent [`chrono`] UTC timestamps.
#[derive(
    Clone,
    Copy,
    Debug,
    Serialize,
    Deserialize,
    SerialEncodable,
    SerialDecodable,
    PartialEq,
    PartialOrd,
    Eq,
)]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// Generate a `Timestamp` of the current time.
    pub fn current_time() -> Self {
        Self(Utc::now().timestamp())
    }

    /// Calculates elapsed time of a `Timestamp`.
    pub fn elapsed(&self) -> u64 {
        let start_time = NaiveDateTime::from_timestamp_opt(self.0, 0).unwrap();
        let end_time = NaiveDateTime::from_timestamp_opt(Utc::now().timestamp(), 0).unwrap();
        let diff = end_time - start_time;
        diff.num_seconds() as u64
    }

    /// Increment a 'Timestamp'.
    pub fn add(&mut self, inc: i64) {
        self.0 += inc;
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let date = timestamp_to_date(self.0, DateFormat::DateTime);
        write!(f, "{}", date)
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Serialize,
    Deserialize,
    SerialEncodable,
    SerialDecodable,
    PartialEq,
    PartialOrd,
    Eq,
)]
pub struct NanoTimestamp(pub i64);

impl NanoTimestamp {
    pub fn current_time() -> Self {
        Self(Utc::now().timestamp_nanos())
    }
}
impl std::fmt::Display for NanoTimestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let date = timestamp_to_date(self.0, DateFormat::Nanos);
        write!(f, "{}", date)
    }
}

pub enum DateFormat {
    Default,
    Date,
    DateTime,
    Nanos,
}

pub fn timestamp_to_date(timestamp: i64, format: DateFormat) -> String {
    if timestamp <= 0 {
        return "".to_string()
    }

    match format {
        DateFormat::Date => NaiveDateTime::from_timestamp_opt(timestamp, 0)
            .unwrap()
            .date()
            .format("%-d %b")
            .to_string(),
        DateFormat::DateTime => NaiveDateTime::from_timestamp_opt(timestamp, 0)
            .unwrap()
            .format("%H:%M:%S %A %-d %B")
            .to_string(),
        DateFormat::Nanos => {
            const A_BILLION: i64 = 1_000_000_000;
            NaiveDateTime::from_timestamp_opt(timestamp / A_BILLION, (timestamp % A_BILLION) as u32)
                .unwrap()
                .format("%H:%M:%S.%f")
                .to_string()
        }
        DateFormat::Default => "".to_string(),
    }
}

pub fn unix_timestamp() -> Result<u64> {
    Ok(UNIX_EPOCH.elapsed()?.as_secs())
}
