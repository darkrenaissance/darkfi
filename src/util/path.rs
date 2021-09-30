use crate::Result;
use std::path::{Path, PathBuf};

pub fn expand_path(path: &str) -> Result<PathBuf> {
    let ret: PathBuf;

    if path.starts_with("~/") {
        let homedir = dirs::home_dir().unwrap();
        let remains = PathBuf::from(path.strip_prefix("~/").unwrap());
        ret = [homedir, remains].iter().collect();
    } else if path.starts_with('~') {
        ret = dirs::home_dir().unwrap();
    } else {
        ret = PathBuf::from(path);
    }

    Ok(ret)
}

pub fn join_config_path(file: &Path) -> Result<PathBuf> {
    let mut path = PathBuf::new();
    let dfi_path = Path::new("darkfi");

    if let Some(v) = dirs::config_dir() {
        path.push(v);
    }

    path.push(dfi_path);
    path.push(file);

    Ok(path)
}
