use std::time::UNIX_EPOCH;

use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    serial::{SerialDecodable, SerialEncodable},
    Result,
};

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
        let start_time = NaiveDateTime::from_timestamp(self.0, 0);
        let end_time = NaiveDateTime::from_timestamp(Utc::now().timestamp(), 0);
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
        DateFormat::Date => {
            NaiveDateTime::from_timestamp(timestamp, 0).date().format("%-d %b").to_string()
        }
        DateFormat::DateTime => {
            NaiveDateTime::from_timestamp(timestamp, 0).format("%H:%M:%S %A %-d %B").to_string()
        }
        DateFormat::Nanos => {
            const A_BILLION: i64 = 1_000_000_000;
            NaiveDateTime::from_timestamp(timestamp / A_BILLION, (timestamp % A_BILLION) as u32)
                .format("%H:%M:%S.%f")
                .to_string()
        }
        DateFormat::Default => "".to_string(),
    }
}

pub fn unix_timestamp() -> Result<u64> {
    Ok(UNIX_EPOCH.elapsed()?.as_secs())
}
