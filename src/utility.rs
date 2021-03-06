use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::seq::SliceRandom;
use smol::{Executor, Task};

//use crate::{net, serial, Channel, ClientProtocol, Result, SlabsManagerSafe};
use crate::{net::messages as net, serial, Result};

pub type AddrsStorage = std::sync::Arc<async_std::sync::Mutex<Vec<SocketAddr>>>;

pub type Clock = std::sync::Arc<AtomicU64>;

pub fn get_current_time() -> u64 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Incorrect system clock: time went backwards");
    let in_ms =
        since_the_epoch.as_secs() * 1000 + since_the_epoch.subsec_nanos() as u64 / 1_000_000;
    return in_ms;
}

pub fn save_to_addrs_store(stored_addrs: &Vec<SocketAddr>) -> Result<()> {
    let mut writer = OpenOptions::new()
        .write(true)
        .create(true)
        .open("addrs.dps")?;
    let buffer = serial::serialize(stored_addrs);
    writer.write_all(&buffer)?;
    Ok(())
}

pub fn load_stored_addrs() -> Result<Vec<SocketAddr>> {
    let mut reader = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("addrs.dps")?;
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    if !buffer.is_empty() {
        let addrs: Vec<SocketAddr> = serial::deserialize(&buffer)?;
        Ok(addrs)
    } else {
        Ok(vec![])
    }
}

