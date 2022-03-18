use async_std::sync::Mutex;
use std::{net::SocketAddr, sync::Arc};

use fxhash::FxHashSet;
use rand::seq::SliceRandom;

/// Pointer to hosts class.
pub type HostsPtr = Arc<Hosts>;

/// Manages a store of network addresses.
pub struct Hosts {
    addrs: Mutex<Vec<SocketAddr>>,
}

impl Hosts {
    /// Create a new host list.
    pub fn new() -> Arc<Self> {
        Arc::new(Self { addrs: Mutex::new(Vec::new()) })
    }

    /// Checks if a host address is in the host list.
    async fn contains(&self, addrs: &[SocketAddr]) -> bool {
        let a_set: FxHashSet<_> = addrs.iter().copied().collect();
        self.addrs.lock().await.iter().any(|item| a_set.contains(item))
    }

    /// Add a new host to the host list.
    pub async fn store(&self, addrs: Vec<SocketAddr>) {
        if !self.contains(&addrs).await {
            self.addrs.lock().await.extend(addrs)
        }
    }

    /// Return a single host address.
    pub async fn load_single(&self) -> Option<SocketAddr> {
        self.addrs.lock().await.choose(&mut rand::thread_rng()).cloned()
    }

    /// Return the list of hosts.
    pub async fn load_all(&self) -> Vec<SocketAddr> {
        self.addrs.lock().await.clone()
    }

    /// Check if the host list is empty.
    pub async fn is_empty(&self) -> bool {
        self.addrs.lock().await.is_empty()
    }
}
