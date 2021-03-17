use async_std::sync::Mutex;
use rand::seq::SliceRandom;
use std::net::SocketAddr;
use std::sync::Arc;
use std::collections::HashSet;

pub type HostsPtr = Arc<Hosts>;

pub struct Hosts {
    addrs: Mutex<Vec<SocketAddr>>,
}

impl Hosts {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            addrs: Mutex::new(Vec::new()),
        })
    }

    async fn contains(&self, addrs: &Vec<SocketAddr>) -> bool {
        let a_set: HashSet<_> = addrs.iter().copied().collect();
        self.addrs.lock().await.iter().any(|item| a_set.contains(item))
    }

    pub async fn store(&self, addrs: Vec<SocketAddr>) {
        if !self.contains(&addrs).await {
            self.addrs.lock().await.extend(addrs)
        }
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

    pub async fn is_empty(&self) -> bool {
        self.addrs.lock().await.is_empty()
    }
}
