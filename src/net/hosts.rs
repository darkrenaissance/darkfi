use async_std::sync::{Arc, Mutex};

use fxhash::FxHashSet;
use url::Url;

/// Pointer to hosts class.
pub type HostsPtr = Arc<Hosts>;

/// Manages a store of network addresses.
pub struct Hosts {
    addrs: Mutex<FxHashSet<Url>>,
}

impl Hosts {
    /// Create a new host list.
    pub fn new() -> Arc<Self> {
        Arc::new(Self { addrs: Mutex::new(FxHashSet::default()) })
    }

    /// Add a new host to the host list.
    pub async fn store(&self, addrs: Vec<Url>) {
        for addr in addrs {
            self.addrs.lock().await.insert(addr);
        }
    }

    /// Return the list of hosts.
    pub async fn load_all(&self) -> Vec<Url> {
        self.addrs.lock().await.iter().cloned().collect()
    }

    /// Remove an Url from the list
    pub async fn remove(&self, url: &Url) -> bool {
        self.addrs.lock().await.remove(url)
    }

    /// Check if the host list is empty.
    pub async fn is_empty(&self) -> bool {
        self.addrs.lock().await.is_empty()
    }
}
