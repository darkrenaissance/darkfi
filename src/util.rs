use std::path::{Path, PathBuf};

use crate::Result;

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
