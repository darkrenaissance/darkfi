/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::{collections::HashSet, sync::Arc};

use log::{debug, trace, warn};
use rand::{
    prelude::{IteratorRandom, SliceRandom},
    rngs::OsRng,
};
use smol::lock::RwLock;
use url::Url;

use super::super::{p2p::P2pPtr, settings::SettingsPtr};
use crate::{
    system::{Subscriber, SubscriberPtr, Subscription},
    Error, Result,
};

/// Atomic pointer to hosts object
pub type HostsPtr = Arc<Hosts>;

// An array containing all possible local host strings
// TODO: This could perhaps be more exhaustive?
pub const LOCAL_HOST_STRS: [&str; 2] = ["localhost", "localhost.localdomain"];

/// Manages a store of network addresses
pub struct Hosts {
    // Intermediary node list that is periodically probed and updated to whitelist.
    pub greylist: RwLock<Vec<(Url, u64)>>,

    // Recently seen nodes.
    pub whitelist: RwLock<Vec<(Url, u64)>>,

    /// Peers we reject from connecting
    rejected: RwLock<HashSet<String>>,

    /// Subscriber listening for store updates
    store_subscriber: SubscriberPtr<usize>,

    /// Pointer to configured P2P settings
    settings: SettingsPtr,
}

impl Hosts {
    /// Create a new hosts list>
    pub fn new(settings: SettingsPtr) -> HostsPtr {
        Arc::new(Self {
            whitelist: RwLock::new(Vec::new()),
            greylist: RwLock::new(Vec::new()),
            rejected: RwLock::new(HashSet::new()),
            store_subscriber: Subscriber::new(),
            settings,
        })
    }

    /// Loops through whitelist addresses to find an outbound address that we can
    /// connect to. Check whether the address is valid by making sure it isn't
    /// our own inbound address, then checks whether it is already connected
    /// (exists) or connecting (pending).
    /// Lastly adds matching address to the pending list.
    pub async fn whitelist_fetch_address_with_lock(
        &self,
        p2p: P2pPtr,
        transports: &[String],
    ) -> Option<(Url, u64)> {
        // Collect hosts
        let mut hosts = vec![];

        // If transport mixing is enabled, then for example we're allowed to
        // use tor:// to connect to tcp:// and tor+tls:// to connect to tcp+tls://.
        // However, **do not** mix tor:// and tcp+tls://, nor tor+tls:// and tcp://.
        let transport_mixing = self.settings.transport_mixing;
        macro_rules! mix_transport {
            ($a:expr, $b:expr) => {
                if transports.contains(&$a.to_string()) && transport_mixing {
                    let mut a_to_b =
                        self.whitelist_fetch_with_schemes(&[$b.to_string()], None).await;
                    for (addr, last_seen) in a_to_b.iter_mut() {
                        addr.set_scheme($a).unwrap();
                        hosts.push((addr.clone(), last_seen.clone()));
                    }
                }
            };
        }
        mix_transport!("tor", "tcp");
        mix_transport!("tor+tls", "tcp+tls");
        mix_transport!("nym", "tcp");
        mix_transport!("nym+tls", "tcp+tls");

        // And now the actual requested transports
        for (addr, last_seen) in self.whitelist_fetch_with_schemes(transports, None).await {
            hosts.push((addr, last_seen));
        }

        // Randomize hosts list. Do not try to connect in a deterministic order.
        // This is healthier for multiple slots to not compete for the same addrs.
        hosts.shuffle(&mut OsRng);

        // Try to find an unused host in the set.
        for (host, last_seen) in hosts.iter() {
            // Check if we already have this connection established
            if p2p.exists(host).await {
                trace!(
                    target: "net::hosts::whitelist_fetch_address_with_lock()",
                    "Host '{}' exists so skipping",
                    host
                );
                continue
            }

            // Check if we already have this configured as a manual peer
            if self.settings.peers.contains(host) {
                trace!(
                    target: "net::hosts::whitelist_fetch_address_with_lock()",
                    "Host '{}' configured as manual peer so skipping",
                    host
                );
                continue
            }

            // Obtain a lock on this address to prevent duplicate connection
            if !p2p.add_pending(host).await {
                trace!(
                    target: "net::hosts::whitelist_fetch_address_with_lock()",
                    "Host '{}' pending so skipping",
                    host
                );
                continue
            }

            trace!(
                target: "net::hosts::whitelist_fetch_address_with_lock()",
                "Found valid host '{}",
                host
            );
            return Some((host.clone(), last_seen.clone()))
        }

        None
    }

    // Store the address in the whitelist if we don't have it.
    // Otherwise, update the last_seen field.
    // TODO: test the performance of this method. It might be costly.
    pub async fn whitelist_store_or_update(&self, addr: &Url, last_seen: u64) -> Result<()> {
        debug!(target: "net::hosts::whitelist_store_or_update()",
        "hosts::whitelist_store_or_update() [START]");

        if !self.whitelist_contains(addr).await {
            self.whitelist_store(addr, last_seen).await;
        } else {
            let index = self.get_whitelist_index_at_addr(addr).await?;
            self.whitelist_update_last_seen(addr, last_seen, index).await;
        }
        Ok(())
    }

    // Update the last_seen field for a Url on the whitelist.
    pub async fn whitelist_update(&self, addr: &Url, last_seen: u64) -> Result<()> {
        let index = self.get_whitelist_index_at_addr(addr).await?;
        self.whitelist_update_last_seen(addr, last_seen, index).await;
        Ok(())
    }

    pub async fn greylist_store_or_update(&self, addrs: &[(Url, u64)]) -> Result<()> {
        debug!(target: "net::hosts::greylist_store_or_update()",
        "hosts::greylist_store_or_update() [START]");

        for (addr, last_seen) in addrs {
            if !self.greylist_contains(addr).await {
                debug!(target: "net::greylist_store_or_update()", "New greylist candidate found!");
                // TODO: clean this up: greylist_store one item at a time
                self.greylist_store(&[(addr.clone(), last_seen.clone())]).await;
            } else {
                debug!(target: "net::greylist_store_or_update()",
                "Existing greylist entry found. Updating last_seen...");

                let index = self.get_greylist_index_at_addr(addr).await?;
                self.greylist_update_last_seen(addr, last_seen.clone(), index).await;
            }
        }
        Ok(())
    }

    // Append host to the greylist. Called on learning of a new peer.
    pub async fn greylist_store(&self, addrs: &[(Url, u64)]) {
        debug!(target: "net::hosts::greylist_store()", "hosts::greylist_store() [START]");

        debug!(target: "net::hosts::greylist_store()", "Filtering addresses...");
        let filtered_addrs = self.filter_addresses(addrs).await;
        let filtered_addrs_len = filtered_addrs.len();

        debug!(target: "net::hosts::greylist_store()", "Filtered addresses.");
        if !filtered_addrs.is_empty() {
            debug!(target: "net::hosts::greylist_store()", "Starting greylist write...");
            let mut greylist = self.greylist.write().await;
            debug!(target: "net::hosts::greylist_store()", "Achieved write lock on greylist!");

            // Remove oldest element if the greylist reaches max size.
            if greylist.len() == 5000 {
                let last_entry = greylist.pop().unwrap();
                debug!(target: "net::hosts::greylist_store()", "Greylist reached max size. Removed {:?}", last_entry);
            } else {
                for (addr, last_seen) in filtered_addrs {
                    debug!(target: "net::hosts::greylist_store()", "Inserting {}", addr);
                    greylist.push((addr.clone(), last_seen.clone()))
                }

                // Sort the list by last_seen.
                greylist.sort_unstable_by_key(|entry| entry.1);
            }
        } else {
            debug!(target: "net::hosts::greylist_store()", "Empty address message...")
        }

        self.store_subscriber.notify(filtered_addrs_len).await;
        debug!(target: "net::hosts::greylist_store()", "hosts::greylist_store() [END]");
    }

    // Append host to the whitelist. Called after a successful interaction with an online peer.
    pub async fn whitelist_store(&self, addr: &Url, last_seen: u64) {
        debug!(target: "net::hosts::whitelist_store()", "hosts::whitelist_store() [START]");

        let mut whitelist = self.whitelist.write().await;

        debug!(target: "net::hosts::whitelist_store()", "Inserting {}. Last seen {:?}", addr, last_seen);

        // Remove oldest element if the whitelist reaches max size.
        if whitelist.len() == 1000 {
            let last_entry = whitelist.pop().unwrap();
            debug!(target: "net::hosts::whitelist_store()", "Whitelist reached max size. Removed {:?}", last_entry);
        }
        whitelist.push((addr.clone(), last_seen));

        // Sort the list by last_seen.
        whitelist.sort_unstable_by_key(|entry| entry.1);

        debug!(target: "net::hosts::whitelist_store()", "hosts::whitelist_store() [END]");
    }

    // Update the last_seen field of a peer on the whitelist.
    pub async fn whitelist_update_last_seen(&self, addr: &Url, last_seen: u64, index: usize) {
        debug!(target: "net::hosts::update_last_seen()", "hosts::update_last_seen() [START]");

        let mut whitelist = self.whitelist.write().await;

        whitelist[index] = (addr.clone(), last_seen);
    }

    // Update the last_seen field of a peer on the greylist.
    pub async fn greylist_update_last_seen(&self, addr: &Url, last_seen: u64, index: usize) {
        debug!(target: "net::hosts::greylist_update_last_seen()", 
               "hosts::greylist_update_last_seen() [START]");

        let mut greylist = self.greylist.write().await;

        greylist[index] = (addr.clone(), last_seen);
    }

    pub async fn whitelist_downgrade(&self, addr: &Url) {
        // First lookup the entry using its addr.
        let mut entry = vec![];

        let whitelist = self.whitelist.read().await;
        for (url, time) in whitelist.iter() {
            if url == addr {
                entry.push((url.clone(), time.clone()));
            }
        }

        // TODO: This is for testing purposes.
        assert!(entry.len() == 1);

        // Remove this item from the whitelist.
        let mut whitelist = self.whitelist.write().await;
        // TODO: test!
        let index = whitelist.iter().position(|x| *x == entry[0]);
        // This should never fail since the entry exists.
        whitelist.remove(index.unwrap());

        // Add it to the greylist.
        let addr = entry[0].0.clone();
        let last_seen = entry[0].1.clone();
        self.greylist_store(&[(addr, last_seen)]).await;
    }

    pub async fn subscribe_store(&self) -> Result<Subscription<usize>> {
        let sub = self.store_subscriber.clone().subscribe().await;
        Ok(sub)
    }

    // Verify whether a URL is local.
    // NOTE: This function is stateless and not specific to
    // `Hosts`. For this reason, it might make more sense
    // to move this function to a more appropriate location
    // in the codebase.
    pub async fn is_local_host(&self, url: Url) -> bool {
        // Reject Urls without host strings.
        if url.host_str().is_none() {
            return false
        }

        // We do this hack in order to parse IPs properly.
        // https://github.com/whatwg/url/issues/749
        let addr = Url::parse(&url.as_str().replace(url.scheme(), "http")).unwrap();
        // Filter private IP ranges
        match addr.host().unwrap() {
            url::Host::Ipv4(ip) => {
                if !ip.is_global() {
                    return true
                }
            }
            url::Host::Ipv6(ip) => {
                if !ip.is_global() {
                    return true
                }
            }
            url::Host::Domain(d) => {
                if LOCAL_HOST_STRS.contains(&d) {
                    return true
                }
            }
        }
        false
    }

    /// Filter given addresses based on certain rulesets and validity.
    async fn filter_addresses(&self, addrs: &[(Url, u64)]) -> Vec<(Url, u64)> {
        debug!(target: "net::hosts::filter_addresses()", "Filtering addrs: {:?}", addrs);
        let mut ret = vec![];
        let localnet = self.settings.localnet;

        'addr_loop: for (addr_, last_seen) in addrs {
            // Validate that the format is `scheme://host_str:port`
            if addr_.host_str().is_none() ||
                addr_.port().is_none() ||
                addr_.cannot_be_a_base() ||
                addr_.path_segments().is_some()
            {
                continue
            }

            if self.is_rejected(addr_).await {
                debug!(target: "net::hosts::filter_addresses()", "Peer {} is rejected", addr_);
                continue
            }

            let host_str = addr_.host_str().unwrap();

            if !localnet {
                // Our own external addresses should never enter the hosts set.
                for ext in &self.settings.external_addrs {
                    if host_str == ext.host_str().unwrap() {
                        continue 'addr_loop
                    }
                }
            }

            // We do this hack in order to parse IPs properly.
            // https://github.com/whatwg/url/issues/749
            let addr = Url::parse(&addr_.as_str().replace(addr_.scheme(), "http")).unwrap();

            // Filter non-global ranges if we're not allowing localnet.
            // Should never be allowed in production, so we don't really care
            // about some of them (e.g. 0.0.0.0, or broadcast, etc.).
            if !localnet && self.is_local_host(addr).await {
                continue
            }

            match addr_.scheme() {
                // Validate that the address is an actual onion.
                #[cfg(feature = "p2p-tor")]
                "tor" | "tor+tls" => {
                    use std::str::FromStr;
                    if tor_hscrypto::pk::HsId::from_str(host_str).is_err() {
                        continue
                    }
                    debug!(target: "net::hosts::filter_addresses()", "[Tor] Valid: {}", host_str);
                }

                #[cfg(feature = "p2p-nym")]
                "nym" | "nym+tls" => continue, // <-- Temp skip

                #[cfg(feature = "p2p-tcp")]
                "tcp" | "tcp+tls" => {
                    debug!(target: "net::hosts::filter_addresses()", "[TCP] Valid: {}", host_str);
                }

                _ => continue,
            }

            ret.push((addr_.clone(), last_seen.clone()));
        }

        ret
    }

    /// Check if a given peer (URL) is in the set of rejected hosts
    pub async fn is_rejected(&self, peer: &Url) -> bool {
        // Skip lookup for UNIX sockets and localhost connections
        // as they should never belong to the list of rejected URLs.
        let Some(hostname) = peer.host_str() else { return false };

        if self.is_local_host(peer.clone()).await {
            return false
        }

        self.rejected.read().await.contains(hostname)
    }

    /// Mark a peer as rejected by adding it to the set of rejected URLs.
    pub async fn mark_rejected(&self, peer: &Url) {
        // We ignore UNIX sockets here so we will just work
        // with stuff that has host_str().
        if let Some(hostname) = peer.host_str() {
            // Localhost connections should not be rejected
            // This however allows any Tor and Nym connections.
            if self.is_local_host(peer.clone()).await {
                return
            }

            self.rejected.write().await.insert(hostname.to_string());
        }
    }

    /// Unmark a rejected peer
    pub async fn unmark_rejected(&self, peer: &Url) {
        if let Some(hostname) = peer.host_str() {
            self.rejected.write().await.remove(hostname);
        }
    }

    /// Check if the greylist is empty.
    pub async fn is_empty_greylist(&self) -> bool {
        self.greylist.read().await.is_empty()
    }

    /// Check if the whitelist is empty.
    pub async fn is_empty_whitelist(&self) -> bool {
        self.whitelist.read().await.is_empty()
    }

    /// Check if host is in the greylist
    pub async fn greylist_contains(&self, addr: &Url) -> bool {
        let greylist = self.greylist.read().await;
        if greylist.iter().any(|(u, _t)| u == addr) {
            return true
        }
        return false
    }

    /// Check if host is in the whitelist
    pub async fn whitelist_contains(&self, addr: &Url) -> bool {
        let whitelist = self.whitelist.read().await;
        if whitelist.iter().any(|(u, _t)| u == addr) {
            return true
        }
        return false
    }

    /// Get the index for a given addr on the whitelist.
    pub async fn get_whitelist_index_at_addr(&self, addr: &Url) -> Result<usize> {
        let whitelist = self.whitelist.read().await;
        for (i, (url, _time)) in whitelist.iter().enumerate() {
            if url == addr {
                return Ok(i)
            }
        }
        return Err(Error::InvalidIndex)
    }

    /// Get the index for a given addr on the greylist.
    pub async fn get_greylist_index_at_addr(&self, addr: &Url) -> Result<usize> {
        let greylist = self.greylist.read().await;
        for (i, (url, _time)) in greylist.iter().enumerate() {
            if url == addr {
                return Ok(i)
            }
        }
        return Err(Error::InvalidIndex)
    }
    /// Return all known whitelisted hosts
    pub async fn whitelist_fetch_all(&self) -> Vec<(Url, u64)> {
        self.whitelist.read().await.iter().cloned().collect()
    }

    /// Get up to n random peers from the whitelist.
    pub async fn fetch_n_random(&self, n: u32) -> Vec<(Url, u64)> {
        let n = n as usize;
        if n == 0 {
            return vec![]
        }
        let addrs = self.whitelist.read().await;
        let urls = addrs.iter().choose_multiple(&mut OsRng, n.min(addrs.len()));
        urls.iter().map(|&url| url.clone()).collect()
    }

    /// Get up to n random whitelisted peers that match the given transport schemes from the hosts set.
    pub async fn whitelist_fetch_n_random_with_schemes(
        &self,
        schemes: &[String],
        n: u32,
    ) -> Vec<(Url, u64)> {
        let n = n as usize;
        if n == 0 {
            return vec![]
        }

        // Retrieve all peers corresponding to that transport schemes
        let hosts = self.whitelist_fetch_with_schemes(schemes, None).await;
        if hosts.is_empty() {
            warn!(target: "store::whitelist_fetch_n_random_with_schemes",
                  "Whitelist is empty! Exiting...");
            return hosts
        }

        // Grab random ones
        let urls = hosts.iter().choose_multiple(&mut OsRng, n.min(hosts.len()));
        urls.iter().map(|&url| url.clone()).collect()
    }

    /// Get up to n random whitelisted peers that don't match the given transport schemes from the hosts set.
    pub async fn whitelist_fetch_n_random_excluding_schemes(
        &self,
        schemes: &[String],
        n: u32,
    ) -> Vec<(Url, u64)> {
        let n = n as usize;
        if n == 0 {
            return vec![]
        }

        // Retrieve all peers not corresponding to that transport schemes
        let hosts = self.whitelist_fetch_excluding_schemes(schemes, None).await;
        if hosts.is_empty() {
            warn!(target: "store::whitelist_fetch_n_random_excluding_schemes",
                  "Whitelist is empty! Exiting...");
            return hosts
        }

        // Grab random ones
        let urls = hosts.iter().choose_multiple(&mut OsRng, n.min(hosts.len()));
        urls.iter().map(|&url| url.clone()).collect()
    }

    /// Get up to limit peers that match the given transport schemes from the whitelist.
    /// If limit was not provided, return all matching peers.
    pub async fn whitelist_fetch_with_schemes(
        &self,
        schemes: &[String],
        limit: Option<usize>,
    ) -> Vec<(Url, u64)> {
        let whitelist = self.whitelist.read().await;
        let mut limit = match limit {
            Some(l) => l.min(whitelist.len()),
            None => whitelist.len(),
        };
        let mut ret = vec![];

        if limit == 0 {
            return ret
        }

        for (addr, last_seen) in whitelist.iter() {
            if schemes.contains(&addr.scheme().to_string()) {
                ret.push((addr.clone(), *last_seen));
                limit -= 1;
                if limit == 0 {
                    return ret
                }
            }
        }

        // If we didn't find any, pick some from the greylist
        if ret.is_empty() {
            for (addr, last_seen) in self.greylist.read().await.iter() {
                if schemes.contains(&addr.scheme().to_string()) {
                    ret.push((addr.clone(), *last_seen));
                    limit -= 1;
                    if limit == 0 {
                        break
                    }
                }
            }
        }

        ret
    }

    /// Get up to limit peers that don't match the given transport schemes from the whitelist.
    /// If limit was not provided, return all matching peers.
    pub async fn whitelist_fetch_excluding_schemes(
        &self,
        schemes: &[String],
        limit: Option<usize>,
    ) -> Vec<(Url, u64)> {
        let addrs = self.whitelist.read().await;
        let mut limit = match limit {
            Some(l) => l.min(addrs.len()),
            None => addrs.len(),
        };
        let mut ret = vec![];

        if limit == 0 {
            return ret
        }

        for (addr, last_seen) in addrs.iter() {
            if !schemes.contains(&addr.scheme().to_string()) {
                ret.push((addr.clone(), *last_seen));
                limit -= 1;
                if limit == 0 {
                    return ret
                }
            }
        }

        // If we didn't find any, pick some from the greylist
        if ret.is_empty() {
            for (addr, last_seen) in self.greylist.read().await.iter() {
                if !schemes.contains(&addr.scheme().to_string()) {
                    ret.push((addr.clone(), *last_seen));
                    limit -= 1;
                    if limit == 0 {
                        break
                    }
                }
            }
        }

        ret
    }
}

#[cfg(test)]
mod tests {
    use super::{super::super::settings::Settings, *};
    use std::time::UNIX_EPOCH;

    #[test]
    fn test_is_local_host() {
        smol::block_on(async {
            let settings = Settings {
                localnet: false,
                external_addrs: vec![
                    Url::parse("tcp://foo.bar:123").unwrap(),
                    Url::parse("tcp://lol.cat:321").unwrap(),
                ],
                ..Default::default()
            };
            let hosts = Hosts::new(Arc::new(settings.clone()));

            let local_hosts: Vec<Url> = vec![
                Url::parse("tcp://localhost").unwrap(),
                Url::parse("tcp://127.0.0.1").unwrap(),
                Url::parse("tcp+tls://[::1]").unwrap(),
                Url::parse("tcp://localhost.localdomain").unwrap(),
                Url::parse("tcp://192.168.10.65").unwrap(),
            ];
            for host in local_hosts {
                eprintln!("{}", host);
                assert!(hosts.is_local_host(host).await);
            }
            let remote_hosts: Vec<Url> = vec![
                Url::parse("https://dyne.org").unwrap(),
                Url::parse("tcp://77.168.10.65:2222").unwrap(),
                Url::parse("tcp://[2345:0425:2CA1:0000:0000:0567:5673:23b5]").unwrap(),
                Url::parse("http://eweiibe6tdjsdprb4px6rqrzzcsi22m4koia44kc5pcjr7nec2rlxyad.onion")
                    .unwrap(),
            ];
            for host in remote_hosts {
                assert!(!(hosts.is_local_host(host).await))
            }
        });
    }

    #[test]
    fn test_greylist_store() {
        let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

        smol::block_on(async {
            let settings = Settings {
                localnet: false,
                external_addrs: vec![
                    Url::parse("tcp://foo.bar:123").unwrap(),
                    Url::parse("tcp://lol.cat:321").unwrap(),
                ],
                ..Default::default()
            };

            let hosts = Hosts::new(Arc::new(settings.clone()));
            let mut external_addrs = vec![];
            for addr in settings.external_addrs {
                external_addrs.push((addr, last_seen))
            }

            hosts.greylist_store(&external_addrs).await;
            assert!(hosts.is_empty_greylist().await);

            let local_hosts = vec![
                (Url::parse("tcp://localhost:3921").unwrap(), last_seen),
                (Url::parse("tor://[::1]:21481").unwrap(), last_seen),
                (Url::parse("tcp://192.168.10.65:311").unwrap(), last_seen),
                (Url::parse("tcp+tls://0.0.0.0:2312").unwrap(), last_seen),
                (Url::parse("tcp://255.255.255.255:2131").unwrap(), last_seen),
            ];
            hosts.greylist_store(&local_hosts).await;
            assert!(hosts.is_empty_greylist().await);

            let remote_hosts = vec![
                (Url::parse("tcp://dark.fi:80").unwrap(), last_seen),
                (Url::parse("tcp://http.cat:401").unwrap(), last_seen),
                (Url::parse("tcp://foo.bar:111").unwrap(), last_seen),
            ];
            hosts.greylist_store(&remote_hosts).await;
            assert!(hosts.greylist_contains(&remote_hosts[0].0).await);
            assert!(hosts.greylist_contains(&remote_hosts[1].0).await);
            assert!(!hosts.greylist_contains(&remote_hosts[2].0).await);
        });
    }

    #[test]
    fn test_whitelist_store() {
        smol::block_on(async {
            let settings = Settings {
                localnet: false,
                external_addrs: vec![
                    Url::parse("tcp://foo.bar:123").unwrap(),
                    Url::parse("tcp://lol.cat:321").unwrap(),
                ],
                ..Default::default()
            };

            let hosts = Hosts::new(Arc::new(settings.clone()));
            assert!(hosts.is_empty_whitelist().await);

            let url = Url::parse("tcp://dark.renaissance:333").unwrap();
            let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

            hosts.whitelist_store(&url, last_seen).await;

            assert!(!hosts.is_empty_whitelist().await);
            assert!(hosts.whitelist_contains(&url).await);
        });
    }
}
