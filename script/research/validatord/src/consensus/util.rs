use chrono::{NaiveDateTime, Utc};

use darkfi::util::serial::{SerialDecodable, SerialEncodable};

/// Serialized blake3 hash bytes for character "⊥"
pub const EMPTY_HASH_BYTES: [u8; 32] = [
    254, 233, 82, 102, 23, 208, 153, 87, 96, 165, 163, 194, 238, 7, 1, 88, 14, 1, 249, 118, 197,
    29, 180, 211, 87, 66, 59, 38, 86, 54, 12, 39,
];

/// Util structure to represend chrono UTC timestamps.
#[derive(Debug, Clone, PartialEq, SerialDecodable, SerialEncodable)]
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
