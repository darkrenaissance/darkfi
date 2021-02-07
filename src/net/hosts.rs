use async_std::sync::Mutex;
use rand::seq::SliceRandom;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::net::SettingsPtr;

pub type HostsPtr = Arc<Hosts>;

pub struct Hosts {
    addrs: Mutex<Vec<SocketAddr>>,
    settings: SettingsPtr,
}

impl Hosts {
    pub fn new(settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self {
            addrs: Mutex::new(Vec::new()),
            settings,
        })
    }

    pub async fn store(&self, addrs: Vec<SocketAddr>) {
        self.addrs.lock().await.extend(addrs)
    }

    pub async fn load_single(&self) -> Option<SocketAddr> {
        self.addrs
            .lock()
            .await
            .choose(&mut rand::thread_rng())
            .cloned()
    }

    pub async fn load_all(&self) -> Vec<SocketAddr> {
        self.addrs.lock().await.clone()
    }
}
