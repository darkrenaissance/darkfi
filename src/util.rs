
use crate::Error;
use crate::Result;

use std::path::PathBuf;

pub fn join_config_path(file: &PathBuf) -> Result<PathBuf> {
    let mut path = dirs::home_dir()
        .ok_or(Error::PathNotFound)?
        .as_path()
        .join(".config/darkfi/");
    path.push(file);
    Ok(path)
}
