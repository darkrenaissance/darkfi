use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{Error, Result};

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

pub fn get_config_path(arg: Option<String>, fallback: &str) -> Result<PathBuf> {
    if arg.is_some() {
        expand_path(&arg.unwrap())
    } else {
        join_config_path(&PathBuf::from(fallback))
    }
}

pub fn load_keypair_to_str(path: PathBuf) -> Result<String> {
    if Path::new(&path).exists() {
        let key = fs::read(&path)?;
        let str_buff = std::str::from_utf8(&key)?;
        Ok(str_buff.to_string())
    } else {
        println!("Could not parse keypair path");
        Err(Error::KeypairPathNotFound)
    }
}
