use async_std::sync::{Arc, Mutex};
use std::net::IpAddr;

use fxhash::{FxHashMap, FxHashSet};
use ipnet::{Ipv4Net, Ipv6Net};
use iprange::IpRange;
use log::debug;
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
        debug!(target: "net", "hosts::store() [Start]");
        let addrs = if !self.localnet {
            let filtered = filter_localnet(input_addrs);
            let filtered = filter_invalid(&self.ipv4_range, &self.ipv6_range, filtered);
            filtered.into_iter().map(|(k, _)| k).collect()
        } else {
            debug!(target: "net", "hosts::store() [Localnet mode, skipping filterring.]");
            input_addrs
        };
        let mut addrs_map = self.addrs.lock().await;
        for addr in addrs {
            addrs_map.insert(addr);
        }
        debug!(target: "net", "hosts::store() [End]");
    }

    /// Add a new hosts external adders to the host list, after filtering and verifying
    /// the address url resolves to the provided connection address.
    pub async fn store_ext(&self, connection_addr: Url, input_addrs: Vec<Url>) {
        debug!(target: "net", "hosts::store_ext() [Start]");
        let addrs = if !self.localnet {
            let filtered = filter_localnet(input_addrs);
            let filtered = filter_invalid(&self.ipv4_range, &self.ipv6_range, filtered);
            filter_non_resolving(connection_addr, filtered)
        } else {
            debug!(target: "net", "hosts::store_ext() [Localnet mode, skipping filterring.]");
            input_addrs
        };
        let mut addrs_map = self.addrs.lock().await;
        for addr in addrs {
            addrs_map.insert(addr);
        }
        debug!(target: "net", "hosts::store_ext() [End]");
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
    debug!(target: "net", "hosts::filter_localnet() [Input addresses: {:?}]", input_addrs);
    let mut filtered = vec![];
    for addr in &input_addrs {
        match addr.host_str() {
            Some(host_str) => {
                if LOCALNET.contains(&host_str) {
                    debug!(target: "net", "hosts::filter_localnet() [Filtered LOCALNET host_str: {}]", host_str);
                    continue
                }
            }
            None => {
                debug!(target: "net", "hosts::filter_localnet() [Filtered None host_str for addr: {}]", addr);
                continue
            }
        }
        filtered.push(addr.clone());
    }
    debug!(target: "net", "hosts::filter_localnet() [Filtered addresses: {:?}]", filtered);
    filtered
}

/// Auxiliary function to filter invalid(unresolvable) hosts.
fn filter_invalid(
    ipv4_range: &IpRange<Ipv4Net>,
    ipv6_range: &IpRange<Ipv6Net>,
    input_addrs: Vec<Url>,
) -> FxHashMap<Url, Vec<IpAddr>> {
    debug!(target: "net", "hosts::filter_invalid() [Input addresses: {:?}]", input_addrs);
    let mut filtered = FxHashMap::default();
    for addr in &input_addrs {
        // Discard domainless Urls
        let domain = match addr.domain() {
            Some(d) => d,
            None => {
                debug!(target: "net", "hosts::filter_invalid() [Filtered domainless url: {}]", addr);
                continue
            }
        };

        // Validate onion domain
        if domain.ends_with(".onion") && is_valid_onion(domain) {
            filtered.insert(addr.clone(), vec![]);
            continue
        }

        // Validate normal domain
        match addr.socket_addrs(|| None) {
            Ok(socket_addrs) => {
                // Check if domain resolved to anything
                if socket_addrs.is_empty() {
                    debug!(target: "net", "hosts::filter_invalid() [Filtered unresolvable url: {}]", addr);
                    continue
                }
                // Checking resolved IP validity
                let mut resolves = vec![];
                for i in socket_addrs {
                    let ip = i.ip();
                    match ip {
                        IpAddr::V4(a) => {
                            if ipv4_range.contains(&a) {
                                debug!(target: "net", "hosts::filter_invalid() [Filtered invalid ip: {}]", a);
                                continue
                            }
                            resolves.push(ip);
                        }
                        IpAddr::V6(a) => {
                            if ipv6_range.contains(&a) {
                                debug!(target: "net", "hosts::filter_invalid() [Filtered invalid ip: {}]", a);
                                continue
                            }
                            resolves.push(ip);
                        }
                    }
                }
                if resolves.is_empty() {
                    debug!(target: "net", "hosts::filter_invalid() [Filtered unresolvable url: {}]", addr);
                    continue
                }
                filtered.insert(addr.clone(), resolves);
            }
            Err(err) => {
                debug!(target: "net", "hosts::filter_invalid() [Filtered Err(socket_addrs) for url {}: {}]", addr, err)
            }
        }
    }
    debug!(target: "net", "hosts::filter_invalid() [Filtered addresses: {:?}]", filtered);
    filtered
}

/// Auxiliary function to filter unresolvable hosts, based on provided connection addr (excluding onion).
fn filter_non_resolving(
    connection_addr: Url,
    input_addrs: FxHashMap<Url, Vec<IpAddr>>,
) -> Vec<Url> {
    debug!(target: "net", "hosts::filter_non_resolving() [Input addresses: {:?}]", input_addrs);
    debug!(target: "net", "hosts::filter_non_resolving() [Connection address: {}]", connection_addr);
    let connection_domain = connection_addr.domain().unwrap();
    // Validate connection onion domain
    if connection_domain.ends_with(".onion") && !is_valid_onion(connection_domain) {
        debug!(target: "net", "hosts::filter_non_resolving() [Tor connection detected, skipping filterring.]");
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
    debug!(target: "net", "hosts::filter_non_resolving() [ipv4_range: {:?}]", ipv4_range);
    debug!(target: "net", "hosts::filter_non_resolving() [ipv6_range: {:?}]", ipv6_range);

    // Filter input addresses
    let mut filtered = vec![];
    for (addr, resolves) in &input_addrs {
        // Keep valid onion domains
        let addr_domain = addr.domain().unwrap();
        if addr_domain.ends_with(".onion") && addr_domain == connection_domain {
            filtered.push(addr.clone());
            continue
        }

        // Checking IP validity
        let mut valid = false;
        for ip in resolves {
            match ip {
                IpAddr::V4(a) => {
                    if ipv4_range.contains(&a) {
                        valid = true;
                        break
                    }
                }
                IpAddr::V6(a) => {
                    if ipv6_range.contains(&a) {
                        valid = true;
                        break
                    }
                }
            }
        }
        if !valid {
            debug!(target: "net", "hosts::filter_non_resolving() [Filtered unresolvable url: {}]", addr);
            continue
        }
        filtered.push(addr.clone());
    }
    debug!(target: "net", "hosts::filter_non_resolving() [Filtered addresses: {:?}]", filtered);
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
