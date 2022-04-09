use chrono::{NaiveDateTime, Utc};
use std::io;

use crate::{
    util::serial::{Decodable, Encodable, ReadExt, SerialDecodable, SerialEncodable, WriteExt},
    Result,
};

// Serialized blake3 hash bytes for character "⊥"
pub const GENESIS_HASH_BYTES: [u8; 32] = [
    254, 233, 82, 102, 23, 208, 153, 87, 96, 165, 163, 194, 238, 7, 1, 88, 14, 1, 249, 118, 197,
    29, 180, 211, 87, 66, 59, 38, 86, 54, 12, 39,
];

/// Util structure to represend chrono UTC timestamps.
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
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

impl Encodable for blake3::Hash {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(self.as_bytes())?;
        Ok(32)
    }
}

impl Decodable for blake3::Hash {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        Ok(bytes.into())
    }
}
