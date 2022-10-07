use async_std::sync::{Arc, Mutex};
use std::net::IpAddr;

use fxhash::FxHashSet;
use ipnet::{Ipv4Net, Ipv6Net};
use iprange::IpRange;
use url::Url;

use super::constants::{IP4_PRIV_RANGES, IP6_PRIV_RANGES, LOCALNET};

/// Pointer to hosts class.
pub type HostsPtr = Arc<Hosts>;

/// Manages a store of network addresses.
pub struct Hosts {
    addrs: Mutex<FxHashSet<Url>>,
    localnet: bool,
    ipv4_range: IpRange<Ipv4Net>,
    ipv6_range: IpRange<Ipv6Net>,
}

impl Hosts {
    /// Create a new host list.
    pub fn new(localnet: bool) -> Arc<Self> {
        // Initialize ipv4_range and ipv6_range if needed
        let mut ipv4_range: IpRange<Ipv4Net> =
            IP4_PRIV_RANGES.iter().map(|s| s.parse().unwrap()).collect();
        let mut ipv6_range: IpRange<Ipv6Net> =
            IP6_PRIV_RANGES.iter().map(|s| s.parse().unwrap()).collect();

        // These will make the trie potentially smaller
        ipv4_range.simplify();
        ipv6_range.simplify();

        Arc::new(Self { addrs: Mutex::new(FxHashSet::default()), localnet, ipv4_range, ipv6_range })
    }

    /// Add a new host to the host list, after filtering.
    pub async fn store(&self, input_addrs: Vec<Url>) {
        let addrs = if !self.localnet {
            let filtered = filter_localnet(input_addrs);
            filter_invalid(&self.ipv4_range, &self.ipv6_range, filtered)
        } else {
            input_addrs
        };
        let mut addrs_map = self.addrs.lock().await;
        for addr in addrs {
            addrs_map.insert(addr);
        }
    }

    /// Add a new hosts external adders to the host list, after filtering and verifying
    /// the address url resolves to the provided connection address.
    pub async fn store_ext(&self, connection_addr: Url, input_addrs: Vec<Url>) {
        let addrs = if !self.localnet {
            let filtered = filter_localnet(input_addrs);
            let filtered = filter_invalid(&self.ipv4_range, &self.ipv6_range, filtered);
            filter_non_resolving(connection_addr, filtered)
        } else {
            input_addrs
        };
        let mut addrs_map = self.addrs.lock().await;
        for addr in addrs {
            addrs_map.insert(addr);
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

/// Auxiliary function to filter localnet hosts.
fn filter_localnet(input_addrs: Vec<Url>) -> Vec<Url> {
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
}

/// Auxiliary function to filter invalid(unresolvable) hosts.
fn filter_invalid(
    ipv4_range: &IpRange<Ipv4Net>,
    ipv6_range: &IpRange<Ipv6Net>,
    input_addrs: Vec<Url>,
) -> Vec<Url> {
    let mut filtered = vec![];
    for addr in &input_addrs {
        // Discard domainless Urls
        let domain = match addr.domain() {
            Some(d) => d,
            None => continue,
        };

        // Validate onion domain
        if domain.ends_with(".onion") && is_valid_onion(domain) {
            filtered.push(addr.clone());
            continue
        }

        // Validate normal domain
        if let Ok(socket_addrs) = addr.socket_addrs(|| None) {
            // Check if domain resolved to anything
            if socket_addrs.is_empty() {
                continue
            }
            // Checking resolved IP validity
            let mut valid = true;
            for i in socket_addrs {
                match i.ip() {
                    IpAddr::V4(a) => {
                        if ipv4_range.contains(&a) {
                            valid = false;
                            break
                        }
                    }
                    IpAddr::V6(a) => {
                        if ipv6_range.contains(&a) {
                            valid = false;
                            break
                        }
                    }
                }
            }
            if valid {
                filtered.push(addr.clone());
            }
        }
    }
    filtered
}

/// Auxiliary function to filter unresolvable hosts, based on provided connection addr (excluding onion).
fn filter_non_resolving(connection_addr: Url, input_addrs: Vec<Url>) -> Vec<Url> {
    let connection_domain = connection_addr.domain().unwrap();
    // Validate connection onion domain
    if connection_domain.ends_with(".onion") && !is_valid_onion(connection_domain) {
        return vec![]
    }

    // Retrieve connection IPs
    let mut ipv4_range = vec![];
    let mut ipv6_range = vec![];
    for i in connection_addr.socket_addrs(|| None).unwrap() {
        match i.ip() {
            IpAddr::V4(a) => {
                ipv4_range.push(a);
            }
            IpAddr::V6(a) => {
                ipv6_range.push(a);
            }
        }
    }

    // Filter input addresses
    let mut filtered = vec![];
    for addr in input_addrs {
        // Keep valid onion domains
        let addr_domain = addr.domain().unwrap();
        if addr_domain.ends_with(".onion") && addr_domain == connection_domain {
            filtered.push(addr.clone());
            continue
        }

        // Checking IP validity
        let mut valid = true;
        let socket_addrs = addr.socket_addrs(|| None).unwrap();
        for i in socket_addrs {
            match i.ip() {
                IpAddr::V4(a) => {
                    if !ipv4_range.contains(&a) {
                        valid = false;
                        break
                    }
                }
                IpAddr::V6(a) => {
                    if !ipv6_range.contains(&a) {
                        valid = false;
                        break
                    }
                }
            }
        }
        if valid {
            filtered.push(addr.clone());
        }
    }
    filtered
}

/// Auxiliary function to validate an onion.
fn is_valid_onion(onion: &str) -> bool {
    let onion = match onion.strip_suffix(".onion") {
        Some(s) => s,
        None => onion,
    };

    if onion.len() != 56 {
        return false
    }

    let alphabet = base32::Alphabet::RFC4648 { padding: false };

    base32::decode(alphabet, onion).is_some()
}
