use chrono::{NaiveDateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{fs::File, io::BufReader, path::PathBuf};

use darkfi::Result;

/// Util function to load a structure saved as a JSON in the provided path file, using serde crate.
pub fn load<T: DeserializeOwned>(path: &PathBuf) -> Result<T> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let value: T = serde_json::from_reader(reader)?;
    Ok(value)
}

/// Util function to save a structure as a JSON in the provided path file, using serde crate.
pub fn save<T: Serialize>(path: &PathBuf, value: &T) -> Result<()> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}

/// Util structure to represend chrono UTC timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
