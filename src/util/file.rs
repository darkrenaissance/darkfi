use std::{fs::File, io::BufReader, path::Path};

use serde::{de::DeserializeOwned, Serialize};

use crate::Result;

pub fn load_json_file<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let value: T = serde_json::from_reader(reader)?;
    Ok(value)
}

pub fn save_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, value)?;
    Ok(())
}
