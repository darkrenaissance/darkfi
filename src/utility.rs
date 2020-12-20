use async_dup::Arc;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::prelude::*;
use std::net::SocketAddr;
use std::sync::atomic::AtomicU64;
use std::time::{SystemTime, UNIX_EPOCH};

use rand::seq::SliceRandom;
use smol::{Executor, Task};

//use crate::{net, serial, Channel, ClientProtocol, Result, SlabsManagerSafe};
use crate::{net::net, serial, Result};

pub type ConnectionsMap = async_dup::Arc<
    async_std::sync::Mutex<HashMap<SocketAddr, async_channel::Sender<net::Message>>>,
>;

pub type AddrsStorage = async_dup::Arc<async_std::sync::Mutex<Vec<SocketAddr>>>;

pub type Clock = async_dup::Arc<AtomicU64>;

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

pub async fn start_connections_process(
    //slabman: SlabsManagerSafe,
    stored_addrs: Vec<SocketAddr>,
    connections: ConnectionsMap,
    _accept_addr: SocketAddr,
    _channel_secret: [u8; 32],
    executor: Arc<Executor<'_>>,
) -> Vec<Task<()>> {
    let mut tasks: Vec<Task<()>> = vec![];
    for _ in 0..10 {
        let connections_cloned = connections.clone();
        let stored_addrs_cloned = stored_addrs.clone();
        //let slabman_cloned = slabman.clone();
        //let channel_secret = channel_secret.clone();
        let task = executor.spawn(async move {
            loop {
                let addr = stored_addrs_cloned.choose(&mut rand::thread_rng()).unwrap();
                if !connections_cloned.lock().await.contains_key(addr) {
                    /*let mut protocol =
                        ClientProtocol::new(connections_cloned.clone(), slabman_cloned.clone());
                    protocol
                        .start(addr.clone(), accept_addr.clone(), &channel_secret)
                        .await;
                        */
                }
            }
        });
        tasks.push(task);
    }
    tasks
}
