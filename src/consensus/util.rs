use chrono::{NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::util::serial::{SerialDecodable, SerialEncodable};

/// Util structure to represend chrono UTC timestamps.
#[derive(Debug, Clone, Serialize, Deserialize, SerialDecodable, SerialEncodable)]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// Calculates elapsed time of a Timestamp.
    pub fn elapsed(self) -> u64 {
        let start_time = NaiveDateTime::from_timestamp(self.0, 0);
        let end_time = NaiveDateTime::from_timestamp(Utc::now().timestamp(), 0);
        let diff = end_time - start_time;
        diff.num_seconds().try_into().unwrap()
    }
}

/// Util function to generate a Timestamp of current time.
pub fn get_current_time() -> Timestamp {
    Timestamp(Utc::now().timestamp())
}
