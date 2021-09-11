use crate::Result;

use std::path::{Path, PathBuf};

pub fn join_config_path(file: &PathBuf) -> Result<PathBuf> {
    let mut path = PathBuf::new();
    let dfi_path = Path::new("darkfi");

    match dirs::config_dir() {
        Some(v) => path.push(v),
        // This should not fail on any modern OS
        None => {}
    }

    path.push(dfi_path);
    path.push(file);

    Ok(path)
}

pub fn hash_to_u64(asset_id: Vec<u8>) -> u64 {
    asset_id.iter().fold(0, |x, &i| x << 8 | i as u64)
}
