use async_std::sync::{Arc, Mutex};

use fxhash::FxHashSet;
use url::Url;

const LOCALNET: [&str; 5] = ["localhost", "0.0.0.0", "[::]", "127.0.0.1", "[::1]"];

/// Pointer to hosts class.
pub type HostsPtr = Arc<Hosts>;

/// Manages a store of network addresses.
pub struct Hosts {
    addrs: Mutex<FxHashSet<Url>>,
    localnet: bool,
}

impl Hosts {
    /// Create a new host list.
    pub fn new(localnet: bool) -> Arc<Self> {
        Arc::new(Self { addrs: Mutex::new(FxHashSet::default()), localnet })
    }

    /// Add a new host to the host list, after filtering localnet hosts,
    /// if configured to do so.
    pub async fn store(&self, input_addrs: Vec<Url>) {
        let addrs = if !self.localnet {
            let mut filtered = vec![];
            for addr in &input_addrs {
                match addr.host_str() {
                    Some(host_str) => {
                        if LOCALNET.contains(&host_str) {
                            continue
                        }
                    }
                    None => continue,
                }
                filtered.push(addr.clone());
            }
            filtered
        } else {
            input_addrs
        };

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
