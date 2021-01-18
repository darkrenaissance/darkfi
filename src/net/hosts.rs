use std::sync::Arc;
use rand::seq::SliceRandom;
use async_std::sync::Mutex;
use std::net::SocketAddr;

use crate::net::SettingsPtr;

pub type HostsPtr = Arc<Hosts>;

pub struct Hosts {
    addrs: Mutex<Vec<SocketAddr>>,
    settings: SettingsPtr
}

impl Hosts {
    pub fn new(settings: SettingsPtr) -> Arc<Self> {
        Arc::new(Self {
            addrs: Mutex::new(Vec::new()),
            settings
        })
    }

    pub async fn store(&self, addrs: Vec<SocketAddr>) {
        self.addrs.lock().await.extend(addrs)
    }

    pub async fn load(&self) -> Option<SocketAddr> {
        self.addrs.lock().await.choose(&mut rand::thread_rng()).cloned()
    }
}

