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

use std::{
    collections::{HashMap, HashSet},
    fs,
    fs::File,
    sync::Arc,
};

use log::{debug, error, info, trace, warn};
use rand::{
    prelude::{IteratorRandom, SliceRandom},
    rngs::OsRng,
    Rng,
};
use smol::lock::RwLock;
use url::Url;

use super::super::{p2p::P2pPtr, settings::SettingsPtr};
use crate::{
    system::{Subscriber, SubscriberPtr, Subscription},
    util::{
        file::{load_file, save_file},
        path::expand_path,
    },
    Error, Result,
};

/// Atomic pointer to hosts object
pub type HostsPtr = Arc<Hosts>;

// An array containing all possible local host strings
// TODO: This could perhaps be more exhaustive?
pub const LOCAL_HOST_STRS: [&str; 2] = ["localhost", "localhost.localdomain"];

const WHITELIST_MAX_LEN: usize = 5000;
const GREYLIST_MAX_LEN: usize = 2000;

/// Manages a store of network addresses
// TODO: Test the performance overhead of using vectors for white/grey/anchor lists.
// TODO: Check whether anchorlist has a max size in Monero.
// TODO: we can probably clean up a lot of the repetitive code in this module.
pub struct Hosts {
    // Intermediary node list that is periodically probed and updated to whitelist.
    pub greylist: RwLock<Vec<(Url, u64)>>,

    // Recently seen nodes.
    pub whitelist: RwLock<Vec<(Url, u64)>>,

    // Nodes to which we have already been able to establish a connection.
    pub anchorlist: RwLock<Vec<(Url, u64)>>,

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
            anchorlist: RwLock::new(Vec::new()),
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
        debug!(target: "store", "whitelist_fetch_address_with_lock() [START]");
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

    pub async fn greylist_fetch_address_with_lock(
        &self,
        p2p: P2pPtr,
        transports: &[String],
    ) -> Option<(Url, u64)> {
        debug!(target: "store", "greylist_fetch_address_with_lock() [START]");
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
                        self.greylist_fetch_with_schemes(&[$b.to_string()], None).await;
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
        for (addr, last_seen) in self.greylist_fetch_with_schemes(transports, None).await {
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
                    target: "net::hosts::greylist_fetch_address_with_lock()",
                    "Host '{}' exists so skipping",
                    host
                );
                continue
            }

            // Check if we already have this configured as a manual peer
            if self.settings.peers.contains(host) {
                trace!(
                    target: "net::hosts::greylist_fetch_address_with_lock()",
                    "Host '{}' configured as manual peer so skipping",
                    host
                );
                continue
            }

            // Obtain a lock on this address to prevent duplicate connection
            if !p2p.add_pending(host).await {
                trace!(
                    target: "net::hosts::greylist_fetch_address_with_lock()",
                    "Host '{}' pending so skipping",
                    host
                );
                continue
            }

            trace!(
                target: "net::hosts::greylist_fetch_address_with_lock()",
                "Found valid host '{}",
                host
            );
            return Some((host.clone(), last_seen.clone()))
        }

        None
    }

    pub async fn anchorlist_fetch_address_with_lock(
        &self,
        p2p: P2pPtr,
        transports: &[String],
    ) -> Option<(Url, u64)> {
        debug!(target: "store", "anchorlist_fetch_address_with_lock() [START]");
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
                        self.anchorlist_fetch_with_schemes(&[$b.to_string()], None).await;
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
        for (addr, last_seen) in self.anchorlist_fetch_with_schemes(transports, None).await {
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
                    target: "net::hosts::anchorlist_fetch_address_with_lock()",
                    "Host '{}' exists so skipping",
                    host
                );
                continue
            }

            // Check if we already have this configured as a manual peer
            if self.settings.peers.contains(host) {
                trace!(
                    target: "net::hosts::anchorlist_fetch_address_with_lock()",
                    "Host '{}' configured as manual peer so skipping",
                    host
                );
                continue
            }

            // Obtain a lock on this address to prevent duplicate connection
            if !p2p.add_pending(host).await {
                trace!(
                    target: "net::hosts::anchorlist_fetch_address_with_lock()",
                    "Host '{}' pending so skipping",
                    host
                );
                continue
            }

            trace!(
                target: "net::hosts::anchorlist_fetch_address_with_lock()",
                "Found valid host '{}",
                host
            );
            return Some((host.clone(), last_seen.clone()))
        }

        None
    }

    // Store the address in the whitelist if we don't have it.
    // Otherwise, update the last_seen field.
    pub async fn whitelist_store_or_update(&self, addrs: &[(Url, u64)]) -> Result<()> {
        debug!(target: "net::hosts::whitelist_store_or_update()", "[START]");

        // No address filtering for whitelist (whitelist is created from greylist)
        for (addr, last_seen) in addrs {
            if !self.whitelist_contains(addr).await {
                debug!(target: "net::hosts::whitelist_store_or_update()",
        "We do not have this entry in the whitelist. Adding to store...");

                self.whitelist_store(addr.clone(), last_seen.clone()).await;
            } else {
                debug!(target: "net::hosts::whitelist_store_or_update()",
        "We have this entry in the whitelist. Updating last seen...");

                let index = self.get_whitelist_index_at_addr(addr).await?;
                self.whitelist_update_last_seen(addr, last_seen.clone(), index).await;
            }
        }
        Ok(())
    }

    pub async fn greylist_store_or_update(&self, addrs: &[(Url, u64)]) -> Result<()> {
        debug!(target: "net::hosts::store::greylist_store_or_update()", "[START]");

        // Filter addresses before writing to the greylist.
        let filtered_addrs = self.filter_addresses(addrs).await;
        let filtered_addrs_len = filtered_addrs.len();
        for (addr, last_seen) in filtered_addrs {
            if !self.hostlist_contains(&addr).await {
                debug!(target: "net::hosts::store::greylist_store_or_update()", "We do not have this entry in the hostlist. Adding to store...");

                self.greylist_store(addr.clone(), last_seen.clone()).await;
            } else {
                debug!(target: "net::hosts::store::greylist_store_or_update()",
                "We have this entry in the greylist. Updating last seen...");

                let index = self.get_greylist_index_at_addr(&addr).await?;
                self.greylist_update_last_seen(&addr, last_seen, index).await;
            }
        }
        self.store_subscriber.notify(filtered_addrs_len).await;
        Ok(())
    }

    pub async fn anchorlist_store_or_update(&self, addrs: &[(Url, u64)]) -> Result<()> {
        debug!(target: "net::hosts::store::anchor_store_or_update()", "[START]");

        // No address filtering for anchorlist (contains addresses we have already connected to)
        for (addr, last_seen) in addrs {
            if !self.anchorlist_contains(addr).await {
                debug!(target: "net::hosts::anchorlist_store_or_update()",
        "We do not have this entry in the whitelist. Adding to store...");

                self.anchorlist_store(addr.clone(), last_seen.clone()).await;
            } else {
                debug!(target: "net::hosts::anchorlist_store_or_update()",
        "We have this entry in the anchorlist. Updating last seen...");

                let (index, entry) = self.get_anchorlist_entry_at_addr(addr).await?;
                self.anchorlist_update_last_seen(addr, last_seen.clone(), index).await;
            }
        }
        Ok(())
    }

    // Append host to the greylist. Called on learning of a new peer.
    pub async fn greylist_store(&self, addr: Url, last_seen: u64) {
        debug!(target: "net::hosts::greylist_store()", "hosts::greylist_store() [START]");

        let mut greylist = self.greylist.write().await;

        // Remove oldest element if the greylist reaches max size.
        if greylist.len() == GREYLIST_MAX_LEN {
            let last_entry = greylist.pop().unwrap();
            debug!(target: "net::hosts::greylist_store()", "Greylist reached max size. Removed {:?}", last_entry);
        } else {
            debug!(target: "net::hosts::greylist_store()", "Inserting {}", addr);
            greylist.push((addr, last_seen));

            // Sort the list by last_seen.
            greylist.sort_by_key(|entry| entry.1);
        }
        debug!(target: "net::hosts::greylist_store()", "[END]");
    }

    // Append host to the whitelist. Called after a successful interaction with an online peer.
    pub async fn whitelist_store(&self, addr: Url, last_seen: u64) {
        debug!(target: "net::hosts::whitelist_store()", "[START]");

        let mut whitelist = self.whitelist.write().await;

        // Remove oldest element if the whitelist reaches max size.
        if whitelist.len() == WHITELIST_MAX_LEN {
            let last_entry = whitelist.pop().unwrap();
            debug!(target: "net::hosts::store::whitelist_store()", "Whitelist reached max size. Removed {:?}", last_entry);
        } else {
            debug!(target: "net::hosts::store::whitelist_store()", "Inserting {}. Last seen {:?}", addr, last_seen);
            whitelist.push((addr, last_seen));

            // Sort the list by last_seen.
            whitelist.sort_by_key(|entry| entry.1);
        }
        debug!(target: "net::hosts::store::whitelist_store()", "[END]");
    }

    pub async fn downgrade_host(&self, addr: &Url) -> Result<()> {
        if self.anchorlist_contains(addr).await {
            debug!(target: "net::store::downgrade_host()", 
                   "Removing non responsive peer from anchorlist");
            let (index, entry) = self.get_anchorlist_entry_at_addr(addr).await?;
            self.anchorlist_remove(addr, index).await;
            self.greylist_store_or_update(&[entry]).await?;

            Ok(())
        } else if self.whitelist_contains(addr).await {
            debug!(target: "net::store::downgrade_host()", 
                   "Removing non responsive peer from whitelist");
            let (index, entry) = self.get_whitelist_entry_at_addr(addr).await?;
            self.whitelist_remove(addr, index).await;
            self.greylist_store_or_update(&[entry]).await?;

            Ok(())
        } else {
            debug!(target: "net::store::downgrade_host()", 
                   "Greylist entry detected! Do nothing for now...");
            let index = self.get_greylist_index_at_addr(addr).await?;
            self.greylist_remove(addr, index).await;

            Ok(())
        }
    }

    // Append host to the anchorlist. Called after we have established a successful connection to a
    // peer.
    pub async fn anchorlist_store(&self, addr: Url, last_seen: u64) {
        debug!(target: "net::hosts::anchorlist_store()", "[START]");

        let mut anchorlist = self.anchorlist.write().await;

        debug!(target: "net::hosts::anchorlist_store()", "Inserting {}", addr);
        anchorlist.push((addr, last_seen));

        // Sort the list by last_seen.
        anchorlist.sort_by_key(|entry| entry.1);
        debug!(target: "net::hosts::anchorlist_store()", "[END]");
    }

    // Update the last_seen field of a peer on the whitelist.
    pub async fn whitelist_update_last_seen(&self, addr: &Url, last_seen: u64, index: usize) {
        debug!(target: "net::hosts::store::whitelist_update_last_seen()", "[START]");

        let mut whitelist = self.whitelist.write().await;

        whitelist[index] = (addr.clone(), last_seen);

        // Sort the list by last_seen.
        whitelist.sort_by_key(|entry| entry.1);

        debug!(target: "net::hosts::store::whitelist_update_last_seen()", "[END]");
    }

    // Update the last_seen field of a peer on the greylist.
    pub async fn greylist_update_last_seen(&self, addr: &Url, last_seen: u64, index: usize) {
        debug!(target: "net::hosts::greylist_update_last_seen()", "[START]");

        let mut greylist = self.greylist.write().await;

        greylist[index] = (addr.clone(), last_seen);

        // Sort the list by last_seen.
        greylist.sort_by_key(|entry| entry.1);

        debug!(target: "net::hosts::store::greylist_update_last_seen()", "[END]");
    }

    // Update the last_seen field of a peer on the anchorlist.
    pub async fn anchorlist_update_last_seen(&self, addr: &Url, last_seen: u64, index: usize) {
        debug!(target: "net::hosts::store::anchorlist_update_last_seen()", "[START]");

        let mut anchorlist = self.anchorlist.write().await;

        anchorlist[index] = (addr.clone(), last_seen);

        // Sort the list by last_seen.
        anchorlist.sort_by_key(|entry| entry.1);

        debug!(target: "net::hosts::store::anchorlist_update_last_seen()", "[END]");
    }

    pub async fn greylist_remove(&self, addr: &Url, position: usize) {
        debug!(target: "net::refinery::run()", "Removing whitelisted peer {} from greylist", addr);
        let mut greylist = self.greylist.write().await;

        greylist.remove(position);

        // Sort the list by last_seen.
        greylist.sort_by_key(|entry| entry.1);
    }

    pub async fn anchorlist_remove(&self, addr: &Url, position: usize) {
        debug!(target: "net::refinery::run()", "Removing disconnected peer {} from anchorlist", addr);
        let mut anchorlist = self.anchorlist.write().await;

        anchorlist.remove(position);

        // Sort the list by last_seen.
        anchorlist.sort_by_key(|entry| entry.1);
    }

    pub async fn whitelist_remove(&self, addr: &Url, position: usize) {
        debug!(target: "net::refinery::run()", "Removing disconnected peer {} from whitelist", addr);
        let mut whitelist = self.whitelist.write().await;

        whitelist.remove(position);

        // Sort the list by last_seen.
        whitelist.sort_by_key(|entry| entry.1);
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
            } else {
                // On localnet, make sure ours ports don't enter the host set.
                for ext in &self.settings.external_addrs {
                    if addr_.port() == ext.port() {
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

    /// Check if the anchorlist is empty.
    pub async fn is_empty_anchorlist(&self) -> bool {
        self.anchorlist.read().await.is_empty()
    }

    /// Check if the hostlist is empty.
    pub async fn is_empty_hostlist(&self) -> bool {
        if self.is_empty_greylist().await &&
            self.is_empty_whitelist().await &&
            self.is_empty_anchorlist().await
        {
            return true
        } else {
            return false
        }
    }

    // Check whether this peer is in any of the hostlists.
    async fn hostlist_contains(&self, addr: &Url) -> bool {
        if self.greylist_contains(addr).await {
            return true
        } else if self.whitelist_contains(addr).await {
            return true
        } else if self.anchorlist_contains(addr).await {
            return true
        } else {
            return false
        }
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

    /// Check if host is in the anchorlist
    pub async fn anchorlist_contains(&self, addr: &Url) -> bool {
        let anchorlist = self.anchorlist.read().await;
        if anchorlist.iter().any(|(u, _t)| u == addr) {
            return true
        }
        return false
    }

    /// Get the index for a given addr on the anchorlist.
    pub async fn get_anchorlist_index_at_addr(&self, addr: &Url) -> Result<usize> {
        let anchorlist = self.anchorlist.read().await;
        for (i, (url, time)) in anchorlist.iter().enumerate() {
            if url == addr {
                return Ok(i)
            }
        }
        return Err(Error::HostDoesNotExist)
    }

    /// Get the index for a given addr on the whitelist.
    pub async fn get_whitelist_index_at_addr(&self, addr: &Url) -> Result<usize> {
        let whitelist = self.whitelist.read().await;
        for (i, (url, _time)) in whitelist.iter().enumerate() {
            if url == addr {
                return Ok(i)
            }
        }
        return Err(Error::HostDoesNotExist)
    }

    /// Get the index for a given addr on the greylist.
    pub async fn get_greylist_index_at_addr(&self, addr: &Url) -> Result<usize> {
        let greylist = self.greylist.read().await;
        for (i, (url, _time)) in greylist.iter().enumerate() {
            if url == addr {
                return Ok(i)
            }
        }
        return Err(Error::HostDoesNotExist)
    }

    /// Get the index and entry for a given addr on the whitelist.
    pub async fn get_whitelist_entry_at_addr(&self, addr: &Url) -> Result<(usize, (Url, u64))> {
        let whitelist = self.whitelist.read().await;
        for (i, (url, time)) in whitelist.iter().enumerate() {
            if url == addr {
                return Ok((i, (url.clone(), time.clone())))
            }
        }
        return Err(Error::HostDoesNotExist)
    }
    /// Get the index and entry for a given addr on the anchorlist.
    pub async fn get_anchorlist_entry_at_addr(&self, addr: &Url) -> Result<(usize, (Url, u64))> {
        let anchorlist = self.anchorlist.read().await;
        for (i, (url, time)) in anchorlist.iter().enumerate() {
            if url == addr {
                return Ok((i, (url.clone(), time.clone())))
            }
        }
        return Err(Error::HostDoesNotExist)
    }

    /// Return all known whitelisted hosts
    pub async fn whitelist_fetch_all(&self) -> Vec<(Url, u64)> {
        self.whitelist.read().await.iter().cloned().collect()
    }

    /// Return all known hosts
    pub async fn hostlist_fetch_all(&self) -> HashMap<String, Vec<(Url, u64)>> {
        let mut hostlist = HashMap::new();
        hostlist.insert(
            "anchorlist".to_string(),
            self.anchorlist.read().await.iter().cloned().collect(),
        );
        hostlist
            .insert("whitelist".to_string(), self.whitelist.read().await.iter().cloned().collect());
        hostlist
            .insert("greylist".to_string(), self.greylist.read().await.iter().cloned().collect());
        hostlist
    }

    /// Get up to n random peers from the whitelist.
    pub async fn whitelist_fetch_n_random(&self, n: u32) -> Vec<(Url, u64)> {
        let n = n as usize;
        if n == 0 {
            return vec![]
        }
        let addrs = self.whitelist.read().await;
        let urls = addrs.iter().choose_multiple(&mut OsRng, n.min(addrs.len()));
        urls.iter().map(|&url| url.clone()).collect()
    }

    pub async fn greylist_fetch_random(&self) -> ((Url, u64), usize) {
        let greylist = self.greylist.read().await;
        let position = rand::thread_rng().gen_range(0..greylist.len());
        let entry = &greylist[position];
        (entry.clone(), position.clone())
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
        debug!(target: "store::whitelist_fetch_n_random_with_schemes", "[START]");

        // Retrieve all peers corresponding to that transport schemes
        let hosts = self.whitelist_fetch_with_schemes(schemes, None).await;
        if hosts.is_empty() {
            debug!(target: "store::whitelist_fetch_n_random_with_schemes",
                  "Whitelist is empty! Exiting...");
            return hosts
        }

        // Grab random ones
        debug!(target: "store::whitelist_fetch_n_random_with_schemes",
               "whitelist is not empty! sending whitelist contents");
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
        debug!(target: "store::whitelist_fetch_excluding_schemes", "[START]");

        // Retrieve all peers not corresponding to that transport schemes
        let hosts = self.whitelist_fetch_excluding_schemes(schemes, None).await;
        if hosts.is_empty() {
            debug!(target: "store::whitelist_fetch_n_random_excluding_schemes",
                  "Whitelist is empty! Exiting...");
            return hosts
        }

        // Grab random ones
        debug!(target: "store::whitelist_fetch_n_random_excluding_schemes",
               "whitelist is not empty! sending whitelist contents");

        let urls = hosts.iter().choose_multiple(&mut OsRng, n.min(hosts.len()));
        urls.iter().map(|&url| url.clone()).collect()
    }

    /// Get up to limit peers that match the given transport schemes from the whitelist.
    /// If limit was not provided, return all matching peers.
    async fn whitelist_fetch_with_schemes(
        &self,
        schemes: &[String],
        limit: Option<usize>,
    ) -> Vec<(Url, u64)> {
        debug!(target: "store::whitelist_fetch_with_schemes", "[START]");
        let mut ret = vec![];

        if !self.is_empty_whitelist().await {
            let whitelist = self.whitelist.read().await;

            let mut limit = match limit {
                Some(l) => l.min(whitelist.len()),
                None => whitelist.len(),
            };

            for (addr, last_seen) in whitelist.iter() {
                if schemes.contains(&addr.scheme().to_string()) {
                    ret.push((addr.clone(), *last_seen));
                    limit -= 1;
                    if limit == 0 {
                        debug!(target: "store::whitelist_fetch_with_schemes",
                           "Found matching white scheme, returning");
                        return ret
                    }
                } else {
                    // TODO: select from greylist?
                    debug!(target: "store::whitelist_fetch_with_schemes",
                          "No matching schemes");
                }
            }
        } else {
            // Whitelist is empty!
            if !self.is_empty_greylist().await {
                // Select from the anchorlist providing it's not empty.
                let greylist = self.greylist.read().await;

                let mut limit = match limit {
                    Some(l) => l.min(greylist.len()),
                    None => greylist.len(),
                };

                for (addr, last_seen) in greylist.iter() {
                    if schemes.contains(&addr.scheme().to_string()) {
                        ret.push((addr.clone(), *last_seen));
                        limit -= 1;
                        if limit == 0 {
                            debug!(target: "store::whitelist_fetch_with_schemes",
                           "Found matching greylist scheme, returning");
                            return ret
                        }
                    } else {
                        debug!(target: "store::whitelist_fetch_with_schemes",
                          "No matching schemes");
                    }
                }
            }
        }

        debug!(target: "store::whitelist_fetch_with_schemes", "END");

        ret
    }

    async fn greylist_fetch_with_schemes(
        &self,
        schemes: &[String],
        limit: Option<usize>,
    ) -> Vec<(Url, u64)> {
        debug!(target: "store::greylist_fetch_with_schemes", "[START]");
        let greylist = self.greylist.read().await;

        let mut limit = match limit {
            Some(l) => l.min(greylist.len()),
            None => greylist.len(),
        };
        let mut ret = vec![];

        if limit == 0 {
            return ret
        }

        for (addr, last_seen) in greylist.iter() {
            if schemes.contains(&addr.scheme().to_string()) {
                ret.push((addr.clone(), *last_seen));
                limit -= 1;
                if limit == 0 {
                    debug!(target: "store::greylist_fetch_with_schemes", "Found matching scheme, returning");
                    return ret
                }
            }
        }

        debug!(target: "store::greylist_fetch_with_schemes", "END");

        ret
    }

    async fn anchorlist_fetch_with_schemes(
        &self,
        schemes: &[String],
        limit: Option<usize>,
    ) -> Vec<(Url, u64)> {
        debug!(target: "store::anchorlist_fetch_with_schemes", "[START]");
        let mut ret = vec![];

        if !self.is_empty_anchorlist().await {
            // Select from the anchorlist providing it's not empty.
            let anchorlist = self.anchorlist.read().await;

            let mut limit = match limit {
                Some(l) => l.min(anchorlist.len()),
                None => anchorlist.len(),
            };

            for (addr, last_seen) in anchorlist.iter() {
                if schemes.contains(&addr.scheme().to_string()) {
                    ret.push((addr.clone(), *last_seen));
                    limit -= 1;
                    if limit == 0 {
                        debug!(target: "store::anchorlist_fetch_with_schemes",
                           "Found matching anchor scheme, returning {:?}", ret);
                        return ret
                    }
                } else {
                    debug!(target: "store::anchorlist_fetch_with_schemes",
                          "No matching schemes!!");
                }
            }
        } else {
            // Anchorlist is empty!
            if !self.is_empty_whitelist().await {
                // Select from the whitelist providing it's not empty.
                let whitelist = self.whitelist.read().await;

                let mut limit = match limit {
                    Some(l) => l.min(whitelist.len()),
                    None => whitelist.len(),
                };

                if limit == 0 {
                    return ret
                }

                for (addr, last_seen) in whitelist.iter() {
                    if schemes.contains(&addr.scheme().to_string()) {
                        ret.push((addr.clone(), *last_seen));
                        limit -= 1;
                        if limit == 0 {
                            debug!(target: "store::anchorlist_fetch_with_schemes",
                           "Found matching white scheme, returning {:?}", ret);
                            return ret
                        }
                    } else {
                        debug!(target: "store::anchorlist_fetch_with_schemes",
                          "No matching schemes!!");
                    }
                }
            } else {
                // Whitelist is empty!
                if !self.is_empty_greylist().await {
                    // Select from the anchorlist providing it's not empty.
                    let greylist = self.greylist.read().await;

                    let mut limit = match limit {
                        Some(l) => l.min(greylist.len()),
                        None => greylist.len(),
                    };

                    for (addr, last_seen) in greylist.iter() {
                        if schemes.contains(&addr.scheme().to_string()) {
                            ret.push((addr.clone(), *last_seen));
                            limit -= 1;
                            if limit == 0 {
                                debug!(target: "store::anchorlist_fetch_with_schemes",
                           "Found matching grey scheme, returning {:?}", ret);
                                return ret
                            }
                        } else {
                            debug!(target: "store::anchorlist_fetch_with_schemes",
                          "No matching schemes!!");
                        }
                    }
                }
            }
        }

        debug!(target: "store::anchorlist_fetch_with_schemes", "END");

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

    pub async fn load_hosts(&self) -> Result<()> {
        let path = expand_path(&self.settings.hostlist)?;

        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            File::create(path.clone())?;
        }

        let contents = load_file(&path);
        if let Err(e) = contents {
            warn!(target: "net::hosts::store", "Failed retrieving saved hosts: {}", e);
            return Ok(())
        }

        for line in contents.unwrap().lines() {
            let data: Vec<&str> = line.split('\t').collect();

            let url = match Url::parse(data[1]) {
                Ok(u) => u,
                Err(e) => {
                    debug!(target: "net::hosts::store", "load_hosts(): Skipping malformed URL {}", e);
                    continue
                }
            };

            let last_seen = match data[2].parse::<u64>() {
                Ok(t) => t,
                Err(e) => {
                    debug!(target: "net::hosts::store", "load_hosts(): Skipping malformed last seen {}", e);
                    continue
                }
            };

            match data[0] {
                "greylist" => {
                    self.greylist_store(url, last_seen).await;
                }
                "whitelist" => {
                    self.whitelist_store(url, last_seen).await;
                }
                "anchorlist" => {
                    self.anchorlist_store(url, last_seen).await;
                }
                _ => {
                    debug!(target: "net::hosts::store", "load_hosts(): Malformed list name...");
                }
            }
        }

        Ok(())
    }

    // Save the hostlist to a file.
    pub async fn save_hosts(&self) -> Result<()> {
        let path = expand_path(&self.settings.hostlist)?;

        let mut tsv = String::new();

        for (name, list) in self.hostlist_fetch_all().await {
            for (url, last_seen) in list {
                tsv.push_str(&format!("{}\t{}\t{}\n", name, url, last_seen));
            }
        }

        if !tsv.eq("") {
            info!(target: "net::hosts::store", "Saving hosts to: {:?}",
                  path);
            if let Err(e) = save_file(&path, &tsv) {
                error!(target: "net::hosts::store", "Failed saving hosts: {}", e);
            }
        }

        Ok(())
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
            for addr in settings.external_addrs {
                hosts.greylist_store(addr, last_seen).await;
            }

            assert!(hosts.is_empty_greylist().await);

            let local_hosts = vec![
                (Url::parse("tcp://localhost:3921").unwrap()),
                (Url::parse("tor://[::1]:21481").unwrap()),
                (Url::parse("tcp://192.168.10.65:311").unwrap()),
                (Url::parse("tcp+tls://0.0.0.0:2312").unwrap()),
                (Url::parse("tcp://255.255.255.255:2131").unwrap()),
            ];

            for host in &local_hosts {
                hosts.greylist_store(host.clone(), last_seen).await;
            }
            assert!(hosts.is_empty_greylist().await);

            let remote_hosts = vec![
                (Url::parse("tcp://dark.fi:80").unwrap()),
                (Url::parse("tcp://http.cat:401").unwrap()),
                (Url::parse("tcp://foo.bar:111").unwrap()),
            ];

            for host in &remote_hosts {
                hosts.greylist_store(host.clone(), last_seen).await;
            }

            assert!(hosts.greylist_contains(&remote_hosts[0]).await);
            assert!(hosts.greylist_contains(&remote_hosts[1]).await);
            assert!(!hosts.greylist_contains(&remote_hosts[2]).await);
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

            hosts.whitelist_store(url.clone(), last_seen).await;

            assert!(!hosts.is_empty_whitelist().await);
            assert!(hosts.whitelist_contains(&url).await);
        });
    }
}
