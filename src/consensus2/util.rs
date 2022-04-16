use chrono::{NaiveDateTime, Utc};

use crate::util::serial::{SerialDecodable, SerialEncodable};

/// Wrapper struct to represent [`chrono`] UTC timestamps.
#[derive(Debug, Copy, Clone, PartialEq, SerialDecodable, SerialEncodable)]
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
}
