use async_std::sync::{Arc, Mutex};
use std::net::IpAddr;

use fxhash::{FxHashMap, FxHashSet};
use ipnet::{Ipv4Net, Ipv6Net};
use iprange::IpRange;
use log::{debug, error, warn};
use url::Url;

use super::constants::{IP4_PRIV_RANGES, IP6_PRIV_RANGES, LOCALNET};
use crate::util::encoding::base32;

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
        if let Some(host_str) = addr.host_str() {
            if !LOCALNET.contains(&host_str) {
                filtered.push(addr.clone());
                continue
            }
            debug!(target: "net", "hosts::filter_localnet() [Filtered localnet addr: {}]", addr);
            continue
        }
        warn!(target: "net", "hosts::filter_localnet() [{} addr.host_str is empty, skipping.]", addr);
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
        if domain.ends_with("onion") {
            match is_valid_onion(domain) {
                true => {
                    filtered.insert(addr.clone(), vec![]);
                }
                false => {
                    warn!(target: "net", "hosts::filter_invalid() [Got invalid onion address: {}]", addr)
                }
            }
            continue
        }

        // Validate Internet domains and IPs. socket_addrs() does a resolution
        // with the local DNS resolver (i.e. /etc/resolv.conf), so the admin has
        // to take care of any DNS leaks by properly configuring their system for
        // DNS resolution.
        if let Ok(socket_addrs) = addr.socket_addrs(|| None) {
            // Check if domain resolved to anything
            if socket_addrs.is_empty() {
                debug!(target: "net", "hosts::filter_invalid() [Filtered unresolvable URL: {}]", addr);
                continue
            }

            // Checking resolved IP validity
            let mut resolves = vec![];
            for i in socket_addrs {
                let ip = i.ip();
                match ip {
                    IpAddr::V4(a) => {
                        if ipv4_range.contains(&a) {
                            debug!(target: "net", "hosts::filter_invalid() [Filtered private-range IPv4: {}]", a);
                            continue
                        }
                    }
                    IpAddr::V6(a) => {
                        if ipv6_range.contains(&a) {
                            debug!(target: "net", "hosts::filter_invalid() [Filtered private range IPv6: {}]", a);
                            continue
                        }
                    }
                }
                resolves.push(ip);
            }

            if resolves.is_empty() {
                debug!(target: "net", "hosts::filter_invalid() [Filtered unresolvable URL: {}]", addr);
                continue
            }

            filtered.insert(addr.clone(), resolves);
        } else {
            warn!(target: "net", "hosts::filter_invalid() [Failed resolving socket_addrs for {}]", addr);
            continue
        }
    }

    debug!(target: "net", "hosts::filter_invalid() [Filtered addresses: {:?}]", filtered);
    filtered
}

/// Filters `input_addrs` keys to whatever has at least one `IpAddr` that is
/// the same as `connection_addr`'s IP address.
/// Skips .onion domains.
fn filter_non_resolving(
    connection_addr: Url,
    input_addrs: FxHashMap<Url, Vec<IpAddr>>,
) -> Vec<Url> {
    debug!(target: "net", "hosts::filter_non_resolving() [Input addresses: {:?}]", input_addrs);
    debug!(target: "net", "hosts::filter_non_resolving() [Connection address: {}]", connection_addr);

    // Retrieve connection IPs
    let mut ipv4_range = vec![];
    let mut ipv6_range = vec![];

    match connection_addr.socket_addrs(|| None) {
        Ok(v) => {
            for i in v {
                match i.ip() {
                    IpAddr::V4(a) => ipv4_range.push(a),
                    IpAddr::V6(a) => ipv6_range.push(a),
                }
            }
        }
        Err(e) => {
            error!(target: "net", "hosts::filter_non_resolving() [Failed resolving connection_addr {}: {}]", connection_addr, e);
            return vec![]
        }
    };

    debug!(target: "net", "hosts::filter_non_resolving() [{} IPv4: {:?}]", connection_addr, ipv4_range);
    debug!(target: "net", "hosts::filter_non_resolving() [{} IPv6: {:?}]", connection_addr, ipv6_range);

    let mut filtered = vec![];
    for (addr, resolves) in &input_addrs {
        // Keep onion domains. It's assumed that the .onion addresses
        // have already been validated.
        let addr_domain = addr.domain().unwrap();
        if addr_domain.ends_with(".onion") {
            filtered.push(addr.clone());
            continue
        }

        // Checking IP validity. If at least one IP matches, we consider it fine.
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

/// Validate a given .onion address. Currently it just checks that the
/// length and encoding are ok, and does not do any deeper check. Should
/// be fixed in the future.
fn is_valid_onion(onion: &str) -> bool {
    let onion = match onion.strip_suffix(".onion") {
        Some(s) => s,
        None => onion,
    };

    if onion.len() != 56 {
        return false
    }

    base32::decode(&onion.to_uppercase()).is_some()
}
