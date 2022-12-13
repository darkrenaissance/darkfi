/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
};

use async_std::sync::{Arc, Mutex};
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
    addrs: Mutex<HashSet<Url>>,
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

        Arc::new(Self { addrs: Mutex::new(HashSet::new()), localnet, ipv4_range, ipv6_range })
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
) -> HashMap<Url, Vec<IpAddr>> {
    debug!(target: "net", "hosts::filter_invalid() [Input addresses: {:?}]", input_addrs);
    let mut filtered = HashMap::new();
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
fn filter_non_resolving(connection_addr: Url, input_addrs: HashMap<Url, Vec<IpAddr>>) -> Vec<Url> {
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
                    if ipv4_range.contains(a) {
                        valid = true;
                        break
                    }
                }
                IpAddr::V6(a) => {
                    if ipv6_range.contains(a) {
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

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        net::{IpAddr, Ipv4Addr},
    };

    use ipnet::{Ipv4Net, Ipv6Net};
    use iprange::IpRange;
    use url::Url;

    use crate::net::{
        constants::{IP4_PRIV_RANGES, IP6_PRIV_RANGES},
        hosts::{filter_invalid, filter_localnet, filter_non_resolving, is_valid_onion},
    };

    #[test]
    fn test_filter_localnet() {
        // Uncomment for inner logging
        /*
        simplelog::TermLogger::init(
            simplelog::LevelFilter::Debug,
            simplelog::Config::default(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        )
        .unwrap();
        */

        // Create addresses to test
        let valid = Url::parse("tls://facebook.com:13333").unwrap();
        let onion = Url::parse(
            "tor://facebookwkhpilnemxj7asaniu7vnjjbiltxjqhye3mhbshg7kx5tfyd.onion:13333",
        )
        .unwrap();
        let localhost = Url::parse("tls://localhost:13333").unwrap();
        let localip = Url::parse("tls://127.0.0.1:13333").unwrap();

        // Create input addresses vector
        let input_addrs = vec![valid.clone(), onion.clone(), localhost, localip];

        // Create expected output addresses vector
        let output_addrs = vec![valid, onion];
        let output_addrs: HashSet<&Url> = HashSet::from_iter(output_addrs.iter());

        // Execute filtering for v4 addr
        let filtered = filter_localnet(input_addrs);
        let filtered: HashSet<&Url> = HashSet::from_iter(filtered.iter());
        // Validate filtered addresses
        assert_eq!(output_addrs, filtered);
    }

    #[test]
    fn test_filter_invalid() {
        // Uncomment for inner logging
        /*
        TermLogger::init(
            LevelFilter::Debug,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        )
        .unwrap();
        */

        // Initialize ipv4_range and ipv6_range if needed
        let mut ipv4_range: IpRange<Ipv4Net> =
            IP4_PRIV_RANGES.iter().map(|s| s.parse().unwrap()).collect();
        let mut ipv6_range: IpRange<Ipv6Net> =
            IP6_PRIV_RANGES.iter().map(|s| s.parse().unwrap()).collect();

        // These will make the trie potentially smaller
        ipv4_range.simplify();
        ipv6_range.simplify();

        // Create addresses to test
        let valid = Url::parse("tls://facebook.com:13333").unwrap();
        let domainless = Url::parse("unix:/run/foo.socket").unwrap();
        let mut hostless = Url::parse("tls://185.60.216.35:13333").unwrap();
        hostless.set_host(None).unwrap();
        let onion = Url::parse(
            "tor://facebookwkhpilnemxj7asaniu7vnjjbiltxjqhye3mhbshg7kx5tfyd.onion:13333",
        )
        .unwrap();
        let invalid_onion =
            Url::parse("tor://facebookwemxj7asaniu7vnjjbiltxjqhye3mhbshg7kx5tfyd.onion:13333")
                .unwrap();

        // Create input addresses vector
        let input_addrs = vec![valid.clone(), domainless, hostless, onion.clone(), invalid_onion];

        // Create expected output addresses vector
        let output_addrs = vec![valid, onion];
        let output_addrs: HashSet<&Url> = HashSet::from_iter(output_addrs.iter());

        // Execute filtering for v4 addr
        let filtered = filter_invalid(&ipv4_range, &ipv6_range, input_addrs);
        let filtered: Vec<Url> = filtered.into_iter().map(|(k, _)| k).collect();
        let filtered: HashSet<&Url> = HashSet::from_iter(filtered.iter());
        // Validate filtered addresses
        assert_eq!(output_addrs, filtered);
    }

    #[test]
    fn test_filter_non_resolving() {
        // Uncomment for inner logging
        /*
        TermLogger::init(
            LevelFilter::Debug,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        )
        .unwrap();
        */

        // Create addresses to test
        let connection_url_v4 = Url::parse("tls://185.60.216.35:13333").unwrap();
        let connection_url_v6 =
            Url::parse("tls://[2a03:2880:f12d:83:face:b00c:0:25de]:13333").unwrap();
        let fake_connection_url = Url::parse("tls://185.199.109.153:13333").unwrap();
        let resolving_url = Url::parse("tls://facebook.com:13333").unwrap();
        let random_url = Url::parse("tls://facebookkk.com:13333").unwrap();
        let onion = Url::parse(
            "tor://facebookwkhpilnemxj7asaniu7vnjjbiltxjqhye3mhbshg7kx5tfyd.onion:13333",
        )
        .unwrap();

        // Create input addresses hashmap, containing created addresses, excluding connection url
        let mut input_addrs = HashMap::new();
        input_addrs.insert(
            resolving_url.clone(),
            vec![
                IpAddr::V4(Ipv4Addr::new(185, 60, 216, 35)),
                "2a03:2880:f12d:83:face:b00c:0:25de".parse().unwrap(),
            ],
        );
        input_addrs.insert(random_url, vec![]);
        input_addrs.insert(onion.clone(), vec![]);

        // Create expected output addresses hashset
        let mut output_addrs = HashMap::new();
        output_addrs.insert(
            resolving_url,
            vec![
                IpAddr::V4(Ipv4Addr::new(185, 60, 216, 35)),
                "2a03:2880:f12d:83:face:b00c:0:25de".parse().unwrap(),
            ],
        );
        output_addrs.insert(onion.clone(), vec![]);
        // Convert hashmap to Vec<Url and then to hashset, to ignore shuffling
        let output_addrs: Vec<Url> = output_addrs.into_iter().map(|(k, _)| k).collect();
        let output_addrs: HashSet<&Url> = HashSet::from_iter(output_addrs.iter());

        let mut fake_output_addrs: HashMap<Url, Vec<Url>> = HashMap::new();
        // Onion addresses don't get filtered, as we can't resolve them
        fake_output_addrs.insert(onion, vec![]);
        let fake_output_addrs: Vec<Url> = fake_output_addrs.into_iter().map(|(k, _)| k).collect();
        let fake_output_addrs: HashSet<&Url> = HashSet::from_iter(fake_output_addrs.iter());

        // Execute filtering for v4 addr
        let filtered = filter_non_resolving(connection_url_v4, input_addrs.clone());
        let filtered = HashSet::from_iter(filtered.iter());
        // Validate filtered addresses
        assert_eq!(output_addrs, filtered);

        // Execute filtering for v6 addr
        let filtered = filter_non_resolving(connection_url_v6, input_addrs.clone());
        let filtered = HashSet::from_iter(filtered.iter());
        assert_eq!(output_addrs, filtered);

        // Execute filtering for fake addr
        let filtered = filter_non_resolving(fake_connection_url, input_addrs);
        let filtered = HashSet::from_iter(filtered.iter());
        assert_eq!(fake_output_addrs, filtered);
    }

    #[test]
    fn test_is_valid_onion() {
        // Valid onion
        assert!(is_valid_onion("facebookwkhpilnemxj7asaniu7vnjjbiltxjqhye3mhbshg7kx5tfyd.onion"),);
        // Valid onion without .onion suffix
        assert!(is_valid_onion("facebookwkhpilnemxj7asaniu7vnjjbiltxjqhye3mhbshg7kx5tfyd"),);
        // Invalid onion
        assert!(!is_valid_onion("facebook.com"));
    }
}
