/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use std::{collections::HashMap, fmt, fs, fs::File, sync::Arc, time::Instant};

use log::{debug, error, info, trace, warn};
use rand::{prelude::IteratorRandom, rngs::OsRng, Rng};
use smol::lock::RwLock;
use url::Url;

use super::super::{settings::SettingsPtr, ChannelPtr};
use crate::{
    system::{Subscriber, SubscriberPtr, Subscription},
    util::{
        file::{load_file, save_file},
        path::expand_path,
    },
    Error, Result,
};

// An array containing all possible local host strings
// TODO: This could perhaps be more exhaustive?
pub const LOCAL_HOST_STRS: [&str; 2] = ["localhost", "localhost.localdomain"];
const WHITELIST_MAX_LEN: usize = 5000;
const GREYLIST_MAX_LEN: usize = 2000;

/// Atomic pointer to hosts object
pub type HostsPtr = Arc<Hosts>;

/// Keeps track of hosts and their current state. Prevents race conditions
/// where multiple threads are simultaenously trying to change the state of
/// a given host.
pub type HostRegistry = RwLock<HashMap<Url, HostState>>;

/// HostState is a set of mutually exclusive states that can be Pending,
/// Connected, Disconnected or Refining. The state is `None` when the
/// corresponding host has been removed from the HostRegistry.
///
///                              +----------+
///                          +-- | refining | --+
///                          |   +----------+   |
///                          |                  |
///                          v                  v
///          +---------+    +-----------+    +------+
///          | pending | -> | connected | -> | None |
///          +---------+    +-----------+    +------+
///               |                             ^
///               |                             |
///               |       +-------------+       |
///               +-----> | downgrading | ------+
///                       +-------------+
///
#[derive(Clone, Debug)]
pub enum HostState {
    /// TODO: doc
    Moving,
    /// Hosts that are being connected to in Outbound and Manual Session.
    Pending,
    /// Hosts that have been successfully connected to.
    Connected(ChannelPtr),
    /// Hosts that are migrating from the greylist to the whitelist or being
    /// removed from the greylist, as defined in `refinery.rs`.
    Refining,
}

impl HostState {
    // Try to change state to Moving. Only possible if this
    // connection is pending i.e. if we are trying to connect to this
    // host.
    fn try_move(&self) -> Result<Self> {
        match self {
            HostState::Pending => Ok(HostState::Moving),
            HostState::Connected(_) => Err(Error::StateBlocked(self.to_string())),
            HostState::Moving => Err(Error::StateBlocked(self.to_string())),
            HostState::Refining => Err(Error::StateBlocked(self.to_string())),
        }
    }

    // Try to change state to Refining. Only possible if we are not yet
    // tracking this host in the HostRegistry.
    fn try_refine(&self) -> Result<Self> {
        match self {
            HostState::Pending => Err(Error::StateBlocked(self.to_string())),
            HostState::Connected(_) => Err(Error::StateBlocked(self.to_string())),
            HostState::Moving => Err(Error::StateBlocked(self.to_string())),
            HostState::Refining => Err(Error::StateBlocked(self.to_string())),
        }
    }

    // Try to change state to Connected. Possible if this peer is
    // currently Pending or being Refined. The latter is necessary since
    // the refinery process requires us to establish a connection to
    // a peer.
    fn try_connect(&self, channel: ChannelPtr) -> Result<Self> {
        match self {
            HostState::Pending => Ok(HostState::Connected(channel)),
            HostState::Connected(_) => Err(Error::StateBlocked(self.to_string())),
            HostState::Moving => Err(Error::StateBlocked(self.to_string())),
            HostState::Refining => Ok(HostState::Connected(channel)),
        }
    }

    // Try to change state to Pending. Only possible if we are not yet
    // tracking this host in the HostRegistry.
    fn try_pending(&self) -> Result<Self> {
        match self {
            HostState::Pending => Err(Error::StateBlocked(self.to_string())),
            HostState::Connected(_) => Err(Error::StateBlocked(self.to_string())),
            HostState::Moving => Err(Error::StateBlocked(self.to_string())),
            HostState::Refining => Err(Error::StateBlocked(self.to_string())),
        }
    }
}
impl fmt::Display for HostState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[repr(u8)]
#[derive(Clone, Debug)]
pub enum HostColor {
    /// Intermediary nodes that are periodically probed and updated to White.
    Grey = 0,
    /// Recently seen hosts. Shared with other nodes.
    White = 1,
    /// Nodes to which we have already been able to establish a connection.
    Gold = 2,
    /// Hostile peers that can neither be connected to nor establish
    /// connections to us for the duration of the program.
    Black = 3,
}

impl TryFrom<usize> for HostColor {
    type Error = Error;

    fn try_from(value: usize) -> Result<Self> {
        match value {
            0 => Ok(HostColor::Grey),
            1 => Ok(HostColor::White),
            2 => Ok(HostColor::Gold),
            3 => Ok(HostColor::Black),
            _ => Err(Error::InvalidHostColor),
        }
    }
}

/// A Container for managing Grey, White, Gold and Black
/// hostlists. Exposes a common interface for writing to and querying
/// hostlists.
// TODO: Currently hosts (aside from hosts on the Black list) are on
// multiple lists at once. This needs to be reconsidered.
// Rethink upgrade/ move methods and consider a single method move() which
// removes from one hostlist and places on another.
// TODO: Verify the performance overhead of using vectors for hostlists.
// TODO: Check whether anchorlist (Gold) has a max size in Monero.
pub struct HostContainer {
    pub hostlists: [RwLock<Vec<(Url, u64)>>; 4],
}

impl HostContainer {
    fn new() -> Self {
        let hostlists: [RwLock<Vec<(Url, u64)>>; 4] = [
            RwLock::new(Vec::new()),
            RwLock::new(Vec::new()),
            RwLock::new(Vec::new()),
            RwLock::new(Vec::new()),
        ];

        Self { hostlists }
    }

    /// Append host to a hostlist.
    pub async fn store(&self, color: usize, addr: Url, last_seen: u64) {
        trace!(target: "net::hosts::store()", "[START] {:?}",
        HostColor::try_from(color).unwrap());

        let mut list = self.hostlists[color].write().await;

        list.push((addr, last_seen));

        if color == 0 && list.len() == GREYLIST_MAX_LEN {
            let last_entry = list.pop().unwrap();
            debug!(
                target: "net::hosts::store()",
                "Greylist reached max size. Removed {:?}", last_entry,
            );
        }

        if color == 1 && list.len() == WHITELIST_MAX_LEN {
            let last_entry = list.pop().unwrap();
            debug!(
                target: "net::hosts::store()",
                "Whitelist reached max size. Removed {:?}", last_entry,
            );
        }

        // Sort the list by last_seen.
        list.sort_by_key(|entry| entry.1);
        list.reverse();

        trace!(target: "net::hosts::store()", "[END] {:?}",
        HostColor::try_from(color).unwrap());
    }

    /// Stores an address on a hostlist or updates its last_seen field if we already
    /// have the address.
    pub async fn store_or_update(&self, color: HostColor, addrs: &[(Url, u64)]) {
        trace!(target: "net::hosts::store_or_update()", "[START] {:?}", color);
        let color = color.clone() as usize;

        for (addr, last_seen) in addrs {
            if !self.contains(color, addr).await {
                debug!(target: "net::hosts::store_or_update()",
                    "We do not have {} in {:?} list. Adding to store...", addr,
                    HostColor::try_from(color).unwrap());

                self.store(color, addr.clone(), *last_seen).await;
            } else {
                debug!(target: "net::hosts::store_or_update()",
                        "We have {} in {:?} list. Updating last seen...", addr,
                        HostColor::try_from(color).unwrap());

                let position = self
                    .get_index_at_addr(color, addr.clone())
                    .await
                    .expect("Expected entry to exist");
                debug!(target: "net::hosts::store_or_update()",
                        "Selected index, updating last seen...");
                self.update_last_seen(color, addr, *last_seen, position).await;
            }
        }
        trace!(target: "net::hosts::store_or_update()", "[END] {:?}", color);
    }

    /// Update the last_seen field of a peer on a hostlist.
    pub async fn update_last_seen(&self, color: usize, addr: &Url, last_seen: u64, index: usize) {
        trace!(target: "net::hosts::update_last_seen()", "[START] {:?}",
        HostColor::try_from(color).unwrap());

        let mut list = self.hostlists[color].write().await;

        list[index] = (addr.clone(), last_seen);

        list.sort_by_key(|entry| entry.1);
        list.reverse();
        trace!(target: "net::hosts::update_last_seen()", "[END] {:?}",
        HostColor::try_from(color).unwrap());
    }

    /// Return all known hosts on a hostlist.
    pub async fn fetch_all(&self, color: HostColor) -> Vec<(Url, u64)> {
        self.hostlists[color as usize].read().await.iter().cloned().collect()
    }

    /// Get the oldest entry from a hostlist.
    pub async fn fetch_last(&self, color: HostColor) -> ((Url, u64), usize) {
        let list = self.hostlists[color as usize].read().await;
        let position = list.len() - 1;
        let entry = &list[position];
        (entry.clone(), position)
    }

    /// TODO: documentation
    pub async fn fetch_address(
        &self,
        color: HostColor,
        transports: &[String],
        transport_mixing: bool,
    ) -> Vec<(Url, u64)> {
        trace!(target: "net::hosts::fetch_address()", "[START] {:?}", color);
        let mut hosts = vec![];
        let index = color as usize;

        // If transport mixing is enabled, then for example we're allowed to
        // use tor:// to connect to tcp:// and tor+tls:// to connect to tcp+tls://.
        // However, **do not** mix tor:// and tcp+tls://, nor tor+tls:// and tcp://.
        macro_rules! mix_transport {
            ($a:expr, $b:expr) => {
                if transports.contains(&$a.to_string()) && transport_mixing {
                    let mut a_to_b = self.fetch_with_schemes(index, &[$b.to_string()], None).await;
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
        for (addr, last_seen) in self.fetch_with_schemes(index, transports, None).await {
            hosts.push((addr, last_seen));
        }

        trace!(target: "net::hosts::fetch_address()", "Grabbed hosts, length: {}", hosts.len());

        hosts
    }

    /// Get up to limit peers that match the given transport schemes from a hostlist.
    /// If limit was not provided, return all matching peers.
    async fn fetch_with_schemes(
        &self,
        color: usize,
        schemes: &[String],
        limit: Option<usize>,
    ) -> Vec<(Url, u64)> {
        trace!(target: "net::hosts::fetch_with_schemes()", "[START] {:?}",
        HostColor::try_from(color).unwrap());

        let list = self.hostlists[color].read().await;

        let mut limit = match limit {
            Some(l) => l.min(list.len()),
            None => list.len(),
        };
        let mut ret = vec![];

        if limit == 0 {
            return ret
        }

        for (addr, last_seen) in list.iter() {
            if schemes.contains(&addr.scheme().to_string()) {
                ret.push((addr.clone(), *last_seen));
                limit -= 1;
                if limit == 0 {
                    debug!(target: "net::hosts::fetch_with_schemes()",
                        "Found matching {:?} scheme, returning {} addresses",
                        HostColor::try_from(color).unwrap(), ret.len());
                    return ret
                }
            }
        }

        if ret.is_empty() {
            debug!(target: "net::hosts::fetch_with_schemes()",
                  "No such {:?} schemes found!", HostColor::try_from(color).unwrap())
        }

        ret
    }

    /// Get up to limit peers that don't match the given transport schemes from a hostlist.
    /// If limit was not provided, return all matching peers.
    pub async fn fetch_excluding_schemes(
        &self,
        color: usize,
        schemes: &[String],
        limit: Option<usize>,
    ) -> Vec<(Url, u64)> {
        trace!(target: "net::hosts::fetch_with_schemes()", "[START] {:?}",
        HostColor::try_from(color).unwrap());

        let list = self.hostlists[color].read().await;

        let mut limit = match limit {
            Some(l) => l.min(list.len()),
            None => list.len(),
        };
        let mut ret = vec![];

        if limit == 0 {
            return ret
        }

        for (addr, last_seen) in list.iter() {
            if !schemes.contains(&addr.scheme().to_string()) {
                ret.push((addr.clone(), *last_seen));
                limit -= 1;
                if limit == 0 {
                    return ret
                }
            }
        }

        if ret.is_empty() {
            debug!(target: "net::hosts::fetch_excluding_schemes()",
                    "No such schemes found!")
        }

        ret
    }

    /// Get a random peer from a hostlist.
    pub async fn fetch_random(&self, color: HostColor) -> ((Url, u64), usize) {
        let list = self.hostlists[color as usize].read().await;
        let position = rand::thread_rng().gen_range(0..list.len());
        let entry = &list[position];
        (entry.clone(), position)
    }

    /// Get a random peer from a hostlist that matches the given transport schemes.
    pub async fn fetch_random_with_schemes(
        &self,
        color: HostColor,
        schemes: &[String],
    ) -> Option<((Url, u64), usize)> {
        // Retrieve all peers corresponding to that transport schemes
        trace!(target: "net::hosts::fetch_random_with_schemes()", "[START] {:?}", color);
        let list = self.fetch_with_schemes(color as usize, schemes, None).await;

        if list.is_empty() {
            return None
        }

        let position = rand::thread_rng().gen_range(0..list.len());
        let entry = &list[position];
        Some((entry.clone(), position))
    }

    /// Get up to n random peers. Schemes are not taken into account.
    pub async fn fetch_n_random(&self, color: HostColor, n: u32) -> Vec<(Url, u64)> {
        trace!(target: "net::hosts::fetch_n_random()", "[START] {:?}", color);
        let n = n as usize;
        if n == 0 {
            return vec![]
        }
        let mut hosts = vec![];

        let list = self.hostlists[color as usize].read().await;

        for (addr, last_seen) in list.iter() {
            hosts.push((addr.clone(), *last_seen));
        }

        if hosts.is_empty() {
            debug!(target: "net::hosts::fetch_n_random()",
                        "No entries found!");
            return hosts
        }

        // Grab random ones
        let urls = hosts.iter().choose_multiple(&mut OsRng, n.min(hosts.len()));
        urls.iter().map(|&url| url.clone()).collect()
    }

    /// Get up to n random peers that match the given transport schemes.
    pub async fn fetch_n_random_with_schemes(
        &self,
        color: HostColor,
        schemes: &[String],
        n: u32,
    ) -> Vec<(Url, u64)> {
        trace!(target: "net::hosts::fetch_n_random_with_schemes()", "[START] {:?}", color);
        let index = color as usize;
        let n = n as usize;
        if n == 0 {
            return vec![]
        }

        // Retrieve all peers corresponding to that transport schemes
        let hosts = self.fetch_with_schemes(index, schemes, None).await;
        if hosts.is_empty() {
            debug!(target: "net::hosts::fetch_n_random_with_schemes()",
                  "No such schemes found!");
            return hosts
        }

        // Grab random ones
        let urls = hosts.iter().choose_multiple(&mut OsRng, n.min(hosts.len()));
        urls.iter().map(|&url| url.clone()).collect()
    }

    /// Get up to n random peers that don't match the given transport schemes from
    /// a hostlist.
    pub async fn fetch_n_random_excluding_schemes(
        &self,
        color: HostColor,
        schemes: &[String],
        n: u32,
    ) -> Vec<(Url, u64)> {
        trace!(target: "net::hosts::fetch_excluding_schemes()", "[START] {:?}", color);
        let index = color as usize;
        let n = n as usize;
        if n == 0 {
            return vec![]
        }
        // Retrieve all peers not corresponding to that transport schemes
        let hosts = self.fetch_excluding_schemes(index, schemes, None).await;

        if hosts.is_empty() {
            debug!(target: "net::hosts::fetch_n_random_excluding_schemes()",
            "No such schemes found!");
            return hosts
        }

        // Grab random ones
        let urls = hosts.iter().choose_multiple(&mut OsRng, n.min(hosts.len()));
        urls.iter().map(|&url| url.clone()).collect()
    }

    /// Remove an entry from a hostlist.
    pub async fn remove(&self, color: HostColor, addr: &Url, index: usize) {
        debug!(target: "net::hosts::remove()", "Removing peer {} from {:?}", addr, color);
        let mut list = self.hostlists[color as usize].write().await;
        list.remove(index);
    }

    /// TODO: documentation
    pub async fn remove_if_exists(&self, color: HostColor, addr: &Url) {
        let index = color.clone() as usize;
        if self.contains(index, addr).await {
            let position =
                self.get_index_at_addr(index, addr.clone()).await.expect("Expected index to exist");
            self.remove(color, addr, position).await;
        }
    }

    /// Check if a hostlist is empty.
    pub async fn is_empty(&self, color: HostColor) -> bool {
        self.hostlists[color as usize].read().await.is_empty()
    }

    /// Check if host is in a hostlist
    pub async fn contains(&self, color: usize, addr: &Url) -> bool {
        self.hostlists[color].read().await.iter().any(|(u, _t)| u == addr)
    }

    /// Get the index for a given addr on a hostlist.
    pub async fn get_index_at_addr(&self, color: usize, addr: Url) -> Option<usize> {
        self.hostlists[color].read().await.iter().position(|a| a.0 == addr)
    }

    /// Get the entry for a given addr on the hostlist.
    pub async fn get_entry_at_addr(&self, color: usize, addr: &Url) -> Option<(Url, u64)> {
        self.hostlists[color]
            .read()
            .await
            .iter()
            .find(|(url, _)| url == addr)
            .map(|(url, time)| (url.clone(), *time))
    }

    /// Load the hostlists from a file.
    pub async fn load_all(&self, path: &str) -> Result<()> {
        let path = expand_path(path)?;

        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            File::create(path.clone())?;
        }

        let contents = load_file(&path);
        if let Err(e) = contents {
            warn!(target: "net::hosts::load_hosts()", "Failed retrieving saved hosts: {}", e);
            return Ok(())
        }

        for line in contents.unwrap().lines() {
            let data: Vec<&str> = line.split('\t').collect();

            let url = match Url::parse(data[1]) {
                Ok(u) => u,
                Err(e) => {
                    debug!(target: "net::hosts::load_hosts()", "Skipping malformed URL {}", e);
                    continue
                }
            };

            let last_seen = match data[2].parse::<u64>() {
                Ok(t) => t,
                Err(e) => {
                    debug!(target: "net::hosts::load_hosts()", "Skipping malformed last seen {}", e);
                    continue
                }
            };

            match data[0] {
                "greylist" => {
                    self.store(HostColor::Grey as usize, url, last_seen).await;
                }
                "whitelist" => {
                    self.store(HostColor::White as usize, url, last_seen).await;
                }
                "anchorlist" => {
                    self.store(HostColor::Gold as usize, url, last_seen).await;
                }
                _ => {
                    debug!(target: "net::hosts::load_hosts()", "Malformed list name...");
                }
            }
        }

        Ok(())
    }

    /// Save the hostlist to a file. Whitelist gets written to the greylist to force
    /// whitelist entries through the refinery on start.
    pub async fn save_all(&self, path: &str) -> Result<()> {
        let path = expand_path(path)?;

        let mut tsv = String::new();
        let mut white = vec![];
        let mut greygold: HashMap<String, Vec<(Url, u64)>> = HashMap::new();

        // First gather all the whitelist entries we don't have in greylist.
        for (url, last_seen) in self.fetch_all(HostColor::White).await {
            if !self.contains(HostColor::Grey as usize, &url).await {
                white.push((url, last_seen))
            }
        }

        // Then gather the greylist and anchorlist entries.
        greygold.insert("anchorlist".to_string(), self.fetch_all(HostColor::Gold).await);
        greygold.insert("greylist".to_string(), self.fetch_all(HostColor::Grey).await);

        // We write whitelist entries to the greylist on p2p.stop() to force
        // them through the refinery on start().
        for (name, mut list) in greygold {
            if name == *"greylist".to_string() {
                list.append(&mut white)
            }
            for (url, last_seen) in list {
                tsv.push_str(&format!("{}\t{}\t{}\n", name, url, last_seen));
            }
        }

        if !tsv.eq("") {
            info!(target: "net::hosts::save_hosts()", "Saving hosts to: {:?}",
                  path);
            if let Err(e) = save_file(&path, &tsv) {
                error!(target: "net::hosts::save_hosts()", "Failed saving hosts: {}", e);
            }
        }

        Ok(())
    }
}

/// TODO: documentation
pub struct Hosts {
    /// Subscriber for notifications of new channels
    channel_subscriber: SubscriberPtr<Result<ChannelPtr>>,

    /// Set of stored addresses that are quarantined.
    /// We quarantine peers we've been unable to connect to, but we keep them
    /// around so we can potentially try them again, up to n tries. This should
    /// be helpful in order to self-heal the p2p connections in case we have an
    /// Internet interrupt (goblins unplugging cables)
    quarantine: RwLock<HashMap<Url, usize>>,

    /// A registry that tracks hosts and their current state.
    registry: HostRegistry,

    /// Subscriber listening for store updates
    store_subscriber: SubscriberPtr<usize>,

    /// Pointer to configured P2P settings
    settings: SettingsPtr,

    pub container: HostContainer,
}

impl Hosts {
    /// Create a new hosts list>
    pub fn new(settings: SettingsPtr) -> HostsPtr {
        Arc::new(Self {
            channel_subscriber: Subscriber::new(),
            quarantine: RwLock::new(HashMap::new()),
            registry: RwLock::new(HashMap::new()),
            store_subscriber: Subscriber::new(),
            settings,
            container: HostContainer::new(),
        })
    }

    /// Safely insert into the HostContainer. Filters the addresses first before storing and
    /// notifies the subscriber. Must be called when first receiving greylist addresses.
    pub async fn insert(&self, color: HostColor, addrs: &[(Url, u64)]) {
        trace!(target: "net::hosts:insert()", "[START]");
        let filtered_addrs = self.filter_addresses(self.settings.clone(), addrs).await;
        let filtered_addrs_len = filtered_addrs.len();

        if filtered_addrs.is_empty() {
            debug!(target: "net::hosts::insert()", "Filtered out all addresses");
        }

        self.container.store_or_update(color, &filtered_addrs).await;
        self.store_subscriber.notify(filtered_addrs_len).await;
    }

    /// Try to update the registry. If the host already exists, try to update its state.
    /// Otherwise add the host to the registry along with its state.
    pub async fn try_register(&self, addr: Url, new_state: HostState) -> Result<HostState> {
        let mut registry = self.registry.write().await;

        if registry.contains_key(&addr) {
            let current_state = registry.get(&addr).unwrap().clone();

            debug!(target: "net::hosts::try_update_registry()",
            "Attempting to update addr={} current_state={}, new_state={}",
            addr, current_state, new_state.to_string());

            let result: Result<HostState> = match new_state {
                HostState::Pending => current_state.try_pending(),
                HostState::Connected(c) => current_state.try_connect(c),
                HostState::Moving => current_state.try_move(),
                HostState::Refining => current_state.try_refine(),
            };

            if let Ok(state) = &result {
                registry.insert(addr.clone(), state.clone());
            }

            result
        } else {
            // We don't know this peer. We can safely update the state.
            registry.insert(addr.clone(), new_state.clone());

            Ok(new_state)
        }
    }

    pub async fn check_address(&self, hosts: Vec<(Url, u64)>) -> Option<(Url, u64)> {
        // Try to find an unused host in the set.
        for (host, last_seen) in hosts {
            debug!(target: "net::hosts::check_address()", "Starting checks");

            if self.try_register(host.clone(), HostState::Pending).await.is_err() {
                continue
            }

            debug!(
                target: "net::hosts::check_address()",
                "Found valid host {}",
                host
            );
            return Some((host.clone(), last_seen))
        }

        None
    }

    /// Remove a host from the HostRegistry. Must be called after move(), when the refinery
    /// process fails, or when a channel stops. Prevents hosts from getting trapped in the
    /// HostState logical machinery.
    pub async fn unregister(&self, addr: &Url) {
        debug!(target: "net::hosts::unregister()", "Removing {} from HostRegistry", addr);
        self.registry.write().await.remove(addr);
    }

    /// Returns the list of connected channels.
    pub async fn channels(&self) -> Vec<ChannelPtr> {
        let registry = self.registry.read().await;
        let mut channels = Vec::new();

        for (_, value) in registry.iter() {
            if let HostState::Connected(c) = value {
                channels.push(c.clone());
            }
        }
        channels
    }

    /// Retrieve a random connected channel
    pub async fn random_channel(&self) -> ChannelPtr {
        let channels = self.channels().await;
        let position = rand::thread_rng().gen_range(0..channels.len());
        channels[position].clone()
    }

    /// Add a channel to the set of connected channels
    pub async fn register_channel(&self, channel: ChannelPtr) -> Result<()> {
        let address = channel.address().clone();

        self.try_register(address.clone(), HostState::Connected(channel.clone())).await?;

        self.channel_subscriber.notify(Ok(channel)).await;
        Ok(())
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
    /// Check whether a URL is local host
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

    /// Filter given addresses based on certain rulesets and validity. Strictly called only on
    /// the first time learning of a new peer.
    async fn filter_addresses(
        &self,
        settings: SettingsPtr,
        addrs: &[(Url, u64)],
    ) -> Vec<(Url, u64)> {
        trace!(target: "net::hosts::filter_addresses()", "Filtering addrs: {:?}", addrs);
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

            // Blacklist peers should never enter the hostlist.
            if self.container.contains(HostColor::Black as usize, addr_).await {
                warn!(target: "net::hosts::filter_addresses()",
                "Peer {} is blacklisted", addr_);
                continue
            }

            // Reject this peer if it's already stored on the hostlist.
            if self.container.contains(HostColor::Gold as usize, addr_).await ||
                self.container.contains(HostColor::White as usize, addr_).await
            {
                debug!(target: "net::hosts::filter_addresses()",
                    "We already have {} in the hostlist. Skipping", addr_);
                continue
            }

            let host_str = addr_.host_str().unwrap();

            if !localnet {
                // Our own external addresses should never enter the hosts set.
                for ext in &settings.external_addrs {
                    if host_str == ext.host_str().unwrap() {
                        continue 'addr_loop
                    }
                }
            }
            // On localnet, make sure ours ports don't enter the host set.
            for ext in &settings.external_addrs {
                if addr_.port() == ext.port() {
                    continue 'addr_loop
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
                    trace!(target: "net::hosts::filter_addresses()",
                    "[Tor] Valid: {}", host_str);
                }

                #[cfg(feature = "p2p-nym")]
                "nym" | "nym+tls" => continue, // <-- Temp skip

                #[cfg(feature = "p2p-tcp")]
                "tcp" | "tcp+tls" => {
                    trace!(target: "net::hosts::filter_addresses()",
                    "[TCP] Valid: {}", host_str);
                }

                _ => continue,
            }

            ret.push((addr_.clone(), *last_seen));
        }

        ret
    }

    /// TODO: documentation
    pub async fn move_host(&self, addr: &Url, last_seen: u64, destination: HostColor) {
        if self.try_register(addr.clone(), HostState::Moving).await.is_err() {
            return
        }

        match destination {
            // Downgrade to grey. Remove from white and gold.
            HostColor::Grey => {
                self.container.remove_if_exists(HostColor::Gold, addr).await;
                self.container.remove_if_exists(HostColor::White, addr).await;

                self.container.store_or_update(HostColor::Grey, &[(addr.clone(), last_seen)]).await;
            }

            // Remove from Greylist, add to Whitelist. Called by the Refinery.
            HostColor::White => {
                self.container.remove_if_exists(HostColor::Grey, addr).await;

                self.container
                    .store_or_update(HostColor::White, &[(addr.clone(), last_seen)])
                    .await;
            }

            // Upgrade to gold. Remove from white or grey.
            HostColor::Gold => {
                self.container.remove_if_exists(HostColor::Grey, addr).await;
                self.container.remove_if_exists(HostColor::White, addr).await;

                self.container.store_or_update(HostColor::Gold, &[(addr.clone(), last_seen)]).await;
            }

            // Move to black. Remove from all other lists.
            HostColor::Black => {
                // We ignore UNIX sockets here so we will just work
                // with stuff that has host_str().
                if let Some(_) = addr.host_str() {
                    // Localhost connections should never enter the blacklist
                    // This however allows any Tor and Nym connections.
                    if self.is_local_host(addr.clone()).await {
                        return
                    }

                    self.container.remove_if_exists(HostColor::Grey, addr).await;
                    self.container.remove_if_exists(HostColor::White, addr).await;
                    self.container.remove_if_exists(HostColor::Gold, addr).await;

                    self.container
                        .store_or_update(HostColor::Black, &[(addr.clone(), last_seen)])
                        .await;
                }
            }
        }

        // Remove this entry from HostRegistry to avoid this host getting
        // stuck in the Moving state.
        self.unregister(&addr).await;
    }

    /// Quarantine a peer.
    /// If they've been quarantined for more than a configured limit, move to greylist.
    pub async fn quarantine(&self, addr: &Url, last_seen: u64) {
        debug!(target: "net::hosts::quarantine()", "Quarantining peer {}", addr);
        let timer = Instant::now();
        let mut q = self.quarantine.write().await;
        if let Some(retries) = q.get_mut(addr) {
            *retries += 1;
            debug!(target: "net::hosts::quarantine()",
            "Peer {} quarantined {} times", addr, retries);
            if *retries == self.settings.hosts_quarantine_limit {
                debug!(target: "net::hosts::quarantine()",
                "Reached quarantine limited after {:?}", timer.elapsed());
                drop(q);

                debug!(target: "net::hosts::quarantine()", "Moving to greylist {}", addr);
                self.move_host(addr, last_seen, HostColor::Grey).await;
            }
        } else {
            debug!(target: "net::hosts::quarantine()", "Added peer {} to quarantine", addr);
            q.insert(addr.clone(), 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use smol::Executor;
    use std::time::UNIX_EPOCH;

    use super::{
        super::super::{settings::Settings, P2p},
        *,
    };
    use crate::{net::hosts::refinery::ping_node, system::sleep};

    #[test]
    fn test_ping_node() {
        smol::block_on(async {
            let settings = Settings {
                localnet: false,
                external_addrs: vec![
                    Url::parse("tcp://foo.bar:123").unwrap(),
                    Url::parse("tcp://lol.cat:321").unwrap(),
                ],
                ..Default::default()
            };

            let ex = Arc::new(Executor::new());
            let p2p = P2p::new(settings, ex.clone()).await;

            let url = Url::parse("tcp://xeno.systems.wtf").unwrap();
            println!("Pinging node...");
            let task = ex.spawn(ping_node(url.clone(), p2p));
            ex.run(task).await;
            println!("Ping node complete!");
        });
    }

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
                assert!(!hosts.is_local_host(host).await)
            }
        });
    }

    #[test]
    fn test_store() {
        let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

        smol::block_on(async {
            let settings = Settings { ..Default::default() };

            let hosts = Hosts::new(Arc::new(settings.clone()));
            let grey_hosts = vec![
                Url::parse("tcp://localhost:3921").unwrap(),
                Url::parse("tor://[::1]:21481").unwrap(),
                Url::parse("tcp://192.168.10.65:311").unwrap(),
                Url::parse("tcp+tls://0.0.0.0:2312").unwrap(),
                Url::parse("tcp://255.255.255.255:2131").unwrap(),
            ];

            for addr in &grey_hosts {
                hosts.container.store(HostColor::Grey as usize, addr.clone(), last_seen).await;
            }
            assert!(!hosts.container.is_empty(HostColor::Grey).await);

            let white_hosts = vec![
                Url::parse("tcp://localhost:3921").unwrap(),
                Url::parse("tor://[::1]:21481").unwrap(),
                Url::parse("tcp://192.168.10.65:311").unwrap(),
                Url::parse("tcp+tls://0.0.0.0:2312").unwrap(),
                Url::parse("tcp://255.255.255.255:2131").unwrap(),
            ];

            for host in &white_hosts {
                hosts.container.store(HostColor::White as usize, host.clone(), last_seen).await;
            }
            assert!(!hosts.container.is_empty(HostColor::White).await);

            let gold_hosts = vec![
                Url::parse("tcp://dark.fi:80").unwrap(),
                Url::parse("tcp://http.cat:401").unwrap(),
                Url::parse("tcp://foo.bar:111").unwrap(),
            ];

            for host in &gold_hosts {
                hosts.container.store(HostColor::Gold as usize, host.clone(), last_seen).await;
            }

            assert!(hosts.container.contains(HostColor::Grey as usize, &grey_hosts[0]).await);
            assert!(hosts.container.contains(HostColor::White as usize, &white_hosts[1]).await);
            assert!(hosts.container.contains(HostColor::Gold as usize, &gold_hosts[2]).await);
        });
    }

    #[test]
    fn test_get_last() {
        smol::block_on(async {
            let settings = Settings { ..Default::default() };
            let hosts = Hosts::new(Arc::new(settings.clone()));

            // Build up a hostlist
            for i in 0..10 {
                sleep(1).await;
                let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();
                let url = Url::parse(&format!("tcp://whitelist{}:123", i)).unwrap();
                hosts.container.store(HostColor::White as usize, url.clone(), last_seen).await;
            }

            for (url, last_seen) in
                hosts.container.hostlists[HostColor::White as usize].read().await.iter()
            {
                println!("{} {}", url, last_seen);
            }

            let (entry, _position) = hosts.container.fetch_last(HostColor::White).await;
            println!("last entry: {} {}", entry.0, entry.1);
        });
    }

    #[test]
    fn test_get_entry() {
        smol::block_on(async {
            let settings = Settings { ..Default::default() };
            let hosts = Hosts::new(Arc::new(settings.clone()));

            let url = Url::parse("tcp://dark.renaissance:333").unwrap();
            let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

            hosts.container.store(HostColor::White as usize, url.clone(), last_seen).await;
            hosts.container.store(HostColor::Gold as usize, url.clone(), last_seen).await;

            assert!(hosts
                .container
                .get_entry_at_addr(HostColor::White as usize, &url)
                .await
                .is_some());
            assert!(hosts
                .container
                .get_entry_at_addr(HostColor::Gold as usize, &url)
                .await
                .is_some());
        });
    }

    #[test]
    fn test_remove() {
        smol::block_on(async {
            let settings = Settings { ..Default::default() };
            let hosts = Hosts::new(Arc::new(settings.clone()));

            let url = Url::parse("tcp://dark.renaissance:333").unwrap();
            let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

            hosts.container.store(HostColor::White as usize, url.clone(), last_seen).await;

            sleep(1).await;

            let url = Url::parse("tcp://milady:333").unwrap();
            let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

            hosts.container.store(HostColor::White as usize, url.clone(), last_seen).await;

            sleep(1).await;

            let url = Url::parse("tcp://king-ted:333").unwrap();
            let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();

            hosts.container.store(HostColor::White as usize, url.clone(), last_seen).await;
            for (url, last_seen) in
                hosts.container.hostlists[HostColor::White as usize].read().await.iter()
            {
                println!("{}, {}", url, last_seen);
            }

            let position = hosts
                .container
                .get_index_at_addr(HostColor::White as usize, url.clone())
                .await
                .unwrap();
            hosts.container.remove(HostColor::White, &url, position).await;
            for (url, last_seen) in
                hosts.container.hostlists[HostColor::White as usize].read().await.iter()
            {
                println!("{}, {}", url, last_seen);
            }
        });
    }

    #[test]
    fn test_fetch_address() {
        smol::block_on(async {
            let mut hostlist = vec![];
            let mut grey_urls = vec![];
            let mut white_urls = vec![];
            let mut anchor_urls = vec![];

            let ex = Arc::new(Executor::new());

            let settings = Settings { ..Default::default() };
            let p2p = P2p::new(settings, ex.clone()).await;
            let hosts = &p2p.hosts().container;

            // Build up a hostlist
            for i in 0..5 {
                let last_seen = UNIX_EPOCH.elapsed().unwrap().as_secs();
                hosts
                    .store(
                        HostColor::Grey as usize,
                        Url::parse(&format!("tcp://greylist{}:123", i)).unwrap(),
                        last_seen,
                    )
                    .await;
                hosts
                    .store(
                        HostColor::White as usize,
                        Url::parse(&format!("tcp://whitelist{}:123", i)).unwrap(),
                        last_seen,
                    )
                    .await;
                hosts
                    .store(
                        HostColor::Gold as usize,
                        Url::parse(&format!("tcp://anchorlist{}:123", i)).unwrap(),
                        last_seen,
                    )
                    .await;

                grey_urls
                    .push((Url::parse(&format!("tcp://greylist{}:123", i)).unwrap(), last_seen));
                white_urls
                    .push((Url::parse(&format!("tcp://whitelist{}:123", i)).unwrap(), last_seen));
                anchor_urls
                    .push((Url::parse(&format!("tcp://anchorlist{}:123", i)).unwrap(), last_seen));
            }

            assert!(!hosts.is_empty(HostColor::Grey).await);
            assert!(!hosts.is_empty(HostColor::White).await);
            assert!(!hosts.is_empty(HostColor::Gold).await);

            let transports = ["tcp".to_string()];
            let white_count =
                p2p.settings().outbound_connections * p2p.settings().white_connection_percent / 100;
            let localnet = true;

            // Simulate the address selection logic found in outbound_session::fetch_address()
            for i in 0..8 {
                if i < p2p.settings().anchor_connection_count {
                    if !hosts.fetch_address(HostColor::Gold, &transports, localnet).await.is_empty()
                    {
                        let addrs =
                            hosts.fetch_address(HostColor::Gold, &transports, localnet).await;
                        hostlist.push(addrs);
                    }

                    if !hosts
                        .fetch_address(HostColor::White, &transports, localnet)
                        .await
                        .is_empty()
                    {
                        let addrs =
                            hosts.fetch_address(HostColor::White, &transports, localnet).await;
                        hostlist.push(addrs);
                    }

                    if !hosts.fetch_address(HostColor::Grey, &transports, localnet).await.is_empty()
                    {
                        let addrs =
                            hosts.fetch_address(HostColor::Grey, &transports, localnet).await;
                        hostlist.push(addrs);
                    }
                } else if i < white_count {
                    if !hosts
                        .fetch_address(HostColor::White, &transports, localnet)
                        .await
                        .is_empty()
                    {
                        let addrs =
                            hosts.fetch_address(HostColor::White, &transports, localnet).await;
                        hostlist.push(addrs);
                    }

                    if !hosts.fetch_address(HostColor::Grey, &transports, localnet).await.is_empty()
                    {
                        let addrs =
                            hosts.fetch_address(HostColor::Grey, &transports, localnet).await;
                        hostlist.push(addrs);
                    }
                } else if !hosts
                    .fetch_address(HostColor::Grey, &transports, localnet)
                    .await
                    .is_empty()
                {
                    let addrs = hosts.fetch_address(HostColor::Grey, &transports, localnet).await;
                    hostlist.push(addrs);
                }
            }

            // Check we're returning the correct addresses.
            anchor_urls.sort();
            white_urls.sort();
            grey_urls.sort();
            hostlist[0].sort();
            hostlist[4].sort();
            hostlist[7].sort();

            assert!(anchor_urls == hostlist[0]);
            assert!(white_urls == hostlist[4]);
            assert!(grey_urls == hostlist[7]);
        })
    }
}
