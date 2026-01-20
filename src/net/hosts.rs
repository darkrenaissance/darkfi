/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Host management for the P2P network.
//!
//! `Hosts` is the main interface managing the registry and container.
//! Filters addresses before storing and publishes events on host/channel changes.
//!
//! `HostRegistry` maps peer addresses to their current `HostState`.
//!
//! `HostContainer` stores the hostlists (Grey, White, Gold, Black, Dark) behind a
//! single lock for atomic cross-list operations.
//!
//! # Host Colors
//!
//! - `Grey`: Recently received hosts pending refinement.
//! - `White`: Hosts that passed refinement successfully.
//! - `Gold`: Hosts we've connected to in OutboundSession.
//! - `Black`: Hostile hosts, blocked for the program duration.
//! - `Dark`: Hosts with unsupported transports. Shared with peers but not used locally.
//!   Cleared daily to avoid propagating stale entries.

use parking_lot::{Mutex, RwLock};
use rand::{prelude::IteratorRandom, rngs::OsRng, Rng};
use smol::lock::RwLock as AsyncRwLock;
use std::{
    collections::HashMap,
    fmt, fs,
    fs::File,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Instant, UNIX_EPOCH},
};
use tracing::{debug, error, warn};
use url::{Host, Url};

use super::{
    session::{SESSION_REFINE, SESSION_SEED},
    settings::Settings,
    ChannelPtr,
};
use crate::{
    system::{Publisher, PublisherPtr, Subscription},
    util::{
        file::{load_file, save_file},
        logger::verbose,
        most_frequent_or_any,
        path::expand_path,
        ringbuffer::RingBuffer,
    },
    Error, Result,
};

pub const LOCAL_HOST_STRS: [&str; 2] = ["localhost", "localhost.localdomain"];

const WHITELIST_MAX_LEN: usize = 5000;
const GREYLIST_MAX_LEN: usize = 2000;
const DARKLIST_MAX_LEN: usize = 1000;
const BLACKLIST_MAX_LEN: usize = 10000;

/// How long a host can remain in Free state before being pruned from the registry.
/// 24 hours is appropriate for long-running daemons.
const REGISTRY_PRUNE_AGE_SECS: u64 = 86400;

pub type HostsPtr = Arc<Hosts>;

/// Mutually exclusive states for host lifecycle management.
///
/// ```text
///                +------+
///                | free |
///                +------+
///                   ^
///                   |
///                   v
///                +------+      +---------+
///       +------> | move | ---> | suspend |
///       |        +------+      +---------+
///       |           |               |        +--------+
///       |           |               v        | insert |
///  +---------+      |          +--------+    +--------+
///  | connect |      |          | refine |        ^
///  +---------+      |          +--------+        |
///       |           v               |            v
///       |     +-----------+         |         +------+
///       +---> | connected | <-------+-------> | free |
///             +-----------+                   +------+
///                   ^
///                   |
///                   v
///                +------+
///                | free |
///                +------+
///
/// ```
#[derive(Clone, Debug)]
pub(crate) enum HostState {
    /// Being inserted into the hostlist.
    Insert,
    /// Being refined (greylist -> whitelist check).
    Refine,
    /// Being connected to in Outbound/Manual Session.
    Connect,
    /// Failed connection, awaiting refinement.
    Suspend,
    /// Successfully connected.
    Connected(ChannelPtr),
    /// Moving between hostlists.
    Move,
    /// Available for any operation. Contains timestamp when freed.
    Free(u64),
}

impl HostState {
    fn try_transition(&self, target: HostState) -> Result<HostState> {
        use HostState::*;

        let allowed = matches!(
            (&target, self),
            (Insert, Free(_)) |
                (Refine, Free(_) | Suspend) |
                (Connect, Free(_)) |
                (Connected(_), Free(_) | Connect | Refine | Move) |
                (Move, Free(_) | Connect | Refine | Connected(_)) |
                (Suspend, Move) |
                (Free(_), _)
        );

        if allowed {
            Ok(target)
        } else {
            Err(Error::HostStateBlocked(self.to_string(), target.to_string()))
        }
    }
}

impl fmt::Display for HostState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HostState::Insert => write!(f, "Insert"),
            HostState::Refine => write!(f, "Refine"),
            HostState::Connect => write!(f, "Connect"),
            HostState::Suspend => write!(f, "Suspend"),
            HostState::Connected(_) => write!(f, "Connected"),
            HostState::Move => write!(f, "Move"),
            HostState::Free(_) => write!(f, "Free"),
        }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// Peers that do not match our accepted transports. We are blind to
    /// these nodes (we do not use them) but we send them around the network
    /// anyway to ensure all transports are propagated.
    Dark = 4,
}

impl HostColor {
    const ALL: [HostColor; 5] =
        [HostColor::Grey, HostColor::White, HostColor::Gold, HostColor::Black, HostColor::Dark];

    fn max_len(self) -> Option<usize> {
        match self {
            HostColor::Grey => Some(GREYLIST_MAX_LEN),
            HostColor::White => Some(WHITELIST_MAX_LEN),
            HostColor::Dark => Some(DARKLIST_MAX_LEN),
            HostColor::Black => Some(BLACKLIST_MAX_LEN),
            HostColor::Gold => None, // Limited by connection slots
        }
    }

    fn name(self) -> &'static str {
        match self {
            HostColor::Grey => "grey",
            HostColor::White => "white",
            HostColor::Gold => "gold",
            HostColor::Black => "black",
            HostColor::Dark => "dark",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "grey" => Some(HostColor::Grey),
            "white" => Some(HostColor::White),
            "gold" => Some(HostColor::Gold),
            "black" => Some(HostColor::Black),
            "dark" => Some(HostColor::Dark),
            _ => None,
        }
    }
}

impl TryFrom<usize> for HostColor {
    type Error = Error;

    fn try_from(value: usize) -> Result<Self> {
        HostColor::ALL.get(value).copied().ok_or(Error::InvalidHostColor)
    }
}

/// Container for all hostlists. Uses a single lock for atomic cross-list operations.
pub struct HostContainer {
    pub(in crate::net) lists: RwLock<[Vec<(Url, u64)>; 5]>,
}

impl HostContainer {
    fn new() -> Self {
        Self { lists: RwLock::new([Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()]) }
    }

    /// Store or update an address on a hostlist.
    pub fn store(&self, color: HostColor, addr: Url, last_seen: u64) {
        let mut lists = self.lists.write();
        let list = &mut lists[color as usize];

        if let Some(entry) = list.iter_mut().find(|(u, _)| *u == addr) {
            entry.1 = last_seen;
        } else {
            list.push((addr, last_seen));
        }
    }

    /// Store, sort by last_seen (descending), and enforce max size.
    pub fn store_and_trim(&self, color: HostColor, addr: Url, last_seen: u64) {
        let mut lists = self.lists.write();
        let list = &mut lists[color as usize];

        if let Some(entry) = list.iter_mut().find(|(u, _)| *u == addr) {
            entry.1 = last_seen;
        } else {
            list.push((addr, last_seen));
        }

        list.sort_by_key(|e| std::cmp::Reverse(e.1));

        if let Some(max) = color.max_len() {
            list.truncate(max);
        }
    }

    /// Remove an address from a hostlist if it exists.
    pub fn remove(&self, color: HostColor, addr: &Url) {
        let mut lists = self.lists.write();
        lists[color as usize].retain(|(u, _)| u != addr);
    }

    /// Check if an address exists in a hostlist.
    pub fn contains(&self, color: HostColor, addr: &Url) -> bool {
        self.lists.read()[color as usize].iter().any(|(u, _)| u == addr)
    }

    /// Check if an address exists in any of the specified hostlists.
    pub fn contains_any(&self, colors: &[HostColor], addr: &Url) -> bool {
        let lists = self.lists.read();
        colors.iter().any(|&c| lists[c as usize].iter().any(|(u, _)| u == addr))
    }

    /// Check if any host with the given hostname exists in the specified lists.
    pub fn contains_hostname(&self, colors: &[HostColor], hostname: &str) -> bool {
        let lists = self.lists.read();
        colors
            .iter()
            .any(|&c| lists[c as usize].iter().any(|(u, _)| u.host_str() == Some(hostname)))
    }

    /// Check if a hostlist is empty.
    pub fn is_empty(&self, color: HostColor) -> bool {
        self.lists.read()[color as usize].is_empty()
    }

    /// Update the last_seen field for an address.
    pub fn update_last_seen(&self, color: HostColor, addr: &Url, last_seen: u64) {
        let mut lists = self.lists.write();
        if let Some(entry) = lists[color as usize].iter_mut().find(|(u, _)| u == addr) {
            entry.1 = last_seen;
        }
    }

    /// Get the last_seen field for an address.
    pub fn get_last_seen(&self, color: HostColor, addr: &Url) -> Option<u64> {
        self.lists.read()[color as usize].iter().find(|(u, _)| u == addr).map(|(_, ls)| *ls)
    }

    /// Return all hosts from a hostlist.
    pub fn fetch_all(&self, color: HostColor) -> Vec<(Url, u64)> {
        self.lists.read()[color as usize].clone()
    }

    /// Get the oldest entry (last in sorted list) from a hostlist.
    pub fn fetch_last(&self, color: HostColor) -> Option<(Url, u64)> {
        self.lists.read()[color as usize].last().cloned()
    }

    /// Get hosts matching the given transport schemes.
    pub fn fetch_with_schemes(
        &self,
        color: HostColor,
        schemes: &[String],
        limit: Option<usize>,
    ) -> Vec<(Url, u64)> {
        let lists = self.lists.read();
        lists[color as usize]
            .iter()
            .filter(|(addr, _)| schemes.contains(&addr.scheme().to_string()))
            .take(limit.unwrap_or(usize::MAX))
            .cloned()
            .collect()
    }

    /// Get hosts NOT matching the given transport schemes.
    pub fn fetch_excluding_schemes(
        &self,
        color: HostColor,
        schemes: &[String],
        limit: Option<usize>,
    ) -> Vec<(Url, u64)> {
        let lists = self.lists.read();
        lists[color as usize]
            .iter()
            .filter(|(addr, _)| !schemes.contains(&addr.scheme().to_string()))
            .take(limit.unwrap_or(usize::MAX))
            .cloned()
            .collect()
    }

    /// Get a random host matching the given schemes.
    pub fn fetch_random_with_schemes(
        &self,
        color: HostColor,
        schemes: &[String],
    ) -> Option<(Url, u64)> {
        let hosts = self.fetch_with_schemes(color, schemes, None);
        if hosts.is_empty() {
            return None
        }
        let idx = rand::thread_rng().gen_range(0..hosts.len());
        Some(hosts[idx].clone())
    }

    /// Get up to n random hosts.
    pub fn fetch_n_random(&self, color: HostColor, n: usize) -> Vec<(Url, u64)> {
        if n == 0 {
            return vec![]
        }
        let lists = self.lists.read();
        lists[color as usize].iter().cloned().choose_multiple(&mut OsRng, n)
    }

    /// Get up to n random hosts matching the given schemes.
    pub fn fetch_n_random_with_schemes(
        &self,
        color: HostColor,
        schemes: &[String],
        n: usize,
    ) -> Vec<(Url, u64)> {
        if n == 0 {
            return vec![]
        }
        let hosts = self.fetch_with_schemes(color, schemes, None);
        hosts.into_iter().choose_multiple(&mut OsRng, n)
    }

    /// Get up to n random hosts NOT matching the given schemes.
    pub fn fetch_n_random_excluding_schemes(
        &self,
        color: HostColor,
        schemes: &[String],
        n: usize,
    ) -> Vec<(Url, u64)> {
        if n == 0 {
            return vec![]
        }
        let hosts = self.fetch_excluding_schemes(color, schemes, None);
        hosts.into_iter().choose_multiple(&mut OsRng, n)
    }

    /// Atomically move a host between lists.
    pub fn move_host(&self, addr: &Url, last_seen: u64, dest: HostColor) -> Result<()> {
        let mut lists = self.lists.write();

        // Remove from source lists based on destination
        match dest {
            HostColor::Grey => {
                lists[HostColor::Gold as usize].retain(|(u, _)| u != addr);
                lists[HostColor::White as usize].retain(|(u, _)| u != addr);
            }
            HostColor::White => {
                lists[HostColor::Grey as usize].retain(|(u, _)| u != addr);
            }
            HostColor::Gold => {
                lists[HostColor::Grey as usize].retain(|(u, _)| u != addr);
                lists[HostColor::White as usize].retain(|(u, _)| u != addr);
            }
            HostColor::Black => {
                lists[HostColor::Grey as usize].retain(|(u, _)| u != addr);
                lists[HostColor::White as usize].retain(|(u, _)| u != addr);
                lists[HostColor::Gold as usize].retain(|(u, _)| u != addr);
            }
            HostColor::Dark => return Err(Error::InvalidHostColor),
        }

        // Add to destination
        let dest_list = &mut lists[dest as usize];
        if let Some(entry) = dest_list.iter_mut().find(|(u, _)| u == addr) {
            entry.1 = last_seen;
        } else {
            dest_list.push((addr.clone(), last_seen));
        }

        // Sort and trim
        dest_list.sort_by_key(|e| std::cmp::Reverse(e.1));
        if let Some(max) = dest.max_len() {
            dest_list.truncate(max);
        }

        Ok(())
    }

    /// Remove entries older than max_age seconds.
    pub fn refresh(&self, color: HostColor, max_age: u64) {
        let now = UNIX_EPOCH.elapsed().unwrap().as_secs();
        let mut lists = self.lists.write();
        let original_len = lists[color as usize].len();

        lists[color as usize].retain(|(addr, last_seen)| {
            // Keep if last_seen is in future (clock skew protection)
            if now < *last_seen {
                return true
            }
            let age = now - last_seen;
            if age <= max_age {
                return true
            }
            debug!(target: "net::hosts::refresh", "Removing {addr} (age: {age}s)");
            false
        });

        let removed = original_len - lists[color as usize].len();
        if removed > 0 {
            debug!(target: "net::hosts::refresh", "Removed {removed} old entries from {:?}", color);
        }
    }

    pub fn load_all(&self, path: &str) -> Result<()> {
        let path = expand_path(path)?;

        if !path.exists() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            File::create(path.clone())?;
        }

        let contents = match load_file(&path) {
            Ok(c) => c,
            Err(e) => {
                warn!(target: "net::hosts::load_all", "[P2P] Failed retrieving saved hosts: {e}");
                return Ok(())
            }
        };

        let mut lists = self.lists.write();

        for line in contents.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() < 3 {
                continue;
            }

            let color = match HostColor::from_name(parts[0]) {
                Some(c) => c,
                None => continue,
            };

            let url = match Url::parse(parts[1]) {
                Ok(u) => u,
                Err(_) => continue,
            };

            let last_seen = match parts[2].parse::<u64>() {
                Ok(t) => t,
                Err(_) => continue,
            };

            let list = &mut lists[color as usize];
            list.push((url, last_seen));
            list.sort_by_key(|e| std::cmp::Reverse(e.1));

            if let Some(max) = color.max_len() {
                list.truncate(max);
            }
        }

        // Refresh dark list (remove entries older than one day)
        drop(lists);
        self.refresh(HostColor::Dark, 86400);

        Ok(())
    }

    pub fn save_all(&self, path: &str) -> Result<()> {
        let path = expand_path(path)?;
        let lists = self.lists.read();

        let mut tsv = String::new();
        for color in [HostColor::Dark, HostColor::Grey, HostColor::White, HostColor::Gold] {
            for (url, last_seen) in &lists[color as usize] {
                tsv.push_str(&format!("{}\t{}\t{}\n", color.name(), url, last_seen));
            }
        }

        if !tsv.is_empty() {
            verbose!(target: "net::hosts::save_all", "[P2P] Saving hosts to: {path:?}");
            if let Err(e) = save_file(&path, &tsv) {
                error!(target: "net::hosts::save_all", "[P2P] Failed saving hosts: {e}");
            }
        }

        Ok(())
    }

    /// Perform transport mixing for a URL, returning alternative connection addresses.
    pub fn mix_host(
        addr: &Url,
        transports: &[String],
        mixed_transports: &[String],
        tor_socks5_proxy: &Option<Url>,
        nym_socks5_proxy: &Option<Url>,
    ) -> Vec<Url> {
        if !mixed_transports.contains(&addr.scheme().to_string()) {
            return vec![]
        }

        let mut hosts = vec![];

        let mix = |scheme: &str, target: &str, hosts: &mut Vec<Url>| {
            if transports.contains(&scheme.to_string()) && addr.scheme() == target {
                let mut url = addr.clone();
                let _ = url.set_scheme(scheme);
                hosts.push(url);
            }
        };

        let mix_socks5 =
            |scheme: &str, target: &str, proxies: &[&Option<Url>], hosts: &mut Vec<Url>| {
                if transports.contains(&scheme.to_string()) && addr.scheme() == target {
                    for proxy in proxies {
                        if let Some(base) = proxy.as_ref() {
                            let mut endpoint = base.clone();
                            endpoint.set_path(&format!(
                                "{}:{}",
                                addr.host().unwrap(),
                                addr.port().unwrap()
                            ));
                            let _ = endpoint.set_scheme(scheme);
                            hosts.push(endpoint);
                        }
                    }
                }
            };

        mix("tor", "tcp", &mut hosts);
        mix("tor+tls", "tcp+tls", &mut hosts);
        mix("nym", "tcp", &mut hosts);
        mix("nym+tls", "tcp+tls", &mut hosts);

        mix_socks5("socks5", "tcp", &[tor_socks5_proxy, nym_socks5_proxy], &mut hosts);
        mix_socks5("socks5+tls", "tcp+tls", &[tor_socks5_proxy, nym_socks5_proxy], &mut hosts);
        mix_socks5("socks5", "tor", &[tor_socks5_proxy], &mut hosts);
        mix_socks5("socks5+tls", "tor+tls", &[tor_socks5_proxy], &mut hosts);

        hosts
    }
}

/// Main interface for host management.
pub struct Hosts {
    /// A registry that tracks hosts and their current state.
    registry: Mutex<HashMap<Url, HostState>>,
    /// Hostlists and associated methods
    pub container: HostContainer,
    /// Publisher listening for store updates
    store_publisher: PublisherPtr<usize>,
    /// Publisher for notifications of new channels
    pub(crate) channel_publisher: PublisherPtr<Result<ChannelPtr>>,
    /// Publisher listening for network disconnects
    pub(crate) disconnect_publisher: PublisherPtr<Error>,
    /// Keeps track of the last time a connection was made.
    pub(crate) last_connection: Mutex<Instant>,
    /// Marker for IPv6 availability
    pub(crate) ipv6_available: AtomicBool,
    /// Auto self discovered addresses. Used for filtering self connections.
    auto_self_addrs: Mutex<RingBuffer<Ipv6Addr, 20>>,
    /// Pointer to configured P2P settings
    settings: Arc<AsyncRwLock<Settings>>,
}

impl Hosts {
    /// Create a new hosts list
    pub(crate) fn new(settings: Arc<AsyncRwLock<Settings>>) -> HostsPtr {
        Arc::new(Self {
            registry: Mutex::new(HashMap::new()),
            container: HostContainer::new(),
            store_publisher: Publisher::new(),
            channel_publisher: Publisher::new(),
            disconnect_publisher: Publisher::new(),
            last_connection: Mutex::new(Instant::now()),
            ipv6_available: AtomicBool::new(true),
            auto_self_addrs: Mutex::new(RingBuffer::new()),
            settings,
        })
    }

    /// Try to register a host with a new state.
    pub(crate) fn try_register(&self, addr: Url, new_state: HostState) -> Result<HostState> {
        let mut registry = self.registry.lock();

        let result = if let Some(current) = registry.get(&addr) {
            current.try_transition(new_state)
        } else {
            Ok(new_state)
        };

        if let Ok(ref state) = result {
            registry.insert(addr, state.clone());
        }

        result
    }

    /// Mark a host as Free.
    pub(crate) fn unregister(&self, addr: &Url) -> Result<()> {
        let age = UNIX_EPOCH.elapsed().unwrap().as_secs();
        self.try_register(addr.clone(), HostState::Free(age))?;
        debug!(target: "net::hosts::unregister", "Unregistered: {addr}");
        Ok(())
    }

    /// Prune stale entries from the registry.
    ///
    /// Removes hosts that have been in `Free` state longer than `REGISTRY_PRUNE_AGE_SECS`.
    /// This prevents unbounded growth of the registry over long-running sessions.
    ///
    /// Returns the number of entries pruned.
    pub fn prune_registry(&self) -> usize {
        let now = UNIX_EPOCH.elapsed().unwrap().as_secs();
        let mut registry = self.registry.lock();
        let before = registry.len();

        registry.retain(|url, state| {
            if let HostState::Free(age) = state {
                let elapsed = now.saturating_sub(*age);
                if elapsed > REGISTRY_PRUNE_AGE_SECS {
                    debug!(
                        target: "net::hosts::prune_registry",
                        "Pruning stale entry {url} (idle for {elapsed}s)",
                    );
                    return false
                }
            }
            true
        });

        let pruned = before - registry.len();
        if pruned > 0 {
            debug!(target: "net::hosts::prune_registry", "Pruned {pruned} stale entries");
        }
        pruned
    }

    /// Check if a host can be refined.
    pub fn refinable(&self, addr: &Url) -> bool {
        let registry = self.registry.lock();
        match registry.get(addr) {
            Some(state) => state.try_transition(HostState::Refine).is_ok(),
            None => true,
        }
    }

    /// Return all connected channels.
    pub fn channels(&self) -> Vec<ChannelPtr> {
        self.registry
            .lock()
            .values()
            .filter_map(
                |state| {
                    if let HostState::Connected(c) = state {
                        Some(c.clone())
                    } else {
                        None
                    }
                },
            )
            .collect()
    }

    /// Return connected peers (excluding seed and refinery connections).
    pub fn peers(&self) -> Vec<ChannelPtr> {
        self.registry
            .lock()
            .values()
            .filter_map(|state| {
                if let HostState::Connected(c) = state {
                    if c.session_type_id() & (SESSION_SEED | SESSION_REFINE) == 0 {
                        return Some(c.clone())
                    }
                }
                None
            })
            .collect()
    }

    /// Get a channel by ID.
    pub fn get_channel(&self, id: u32) -> Option<ChannelPtr> {
        self.channels().into_iter().find(|c| c.info.id == id)
    }

    /// Get a random connected channel.
    pub fn random_channel(&self) -> Option<ChannelPtr> {
        let channels = self.channels();
        if channels.is_empty() {
            return None
        }
        let idx = rand::thread_rng().gen_range(0..channels.len());
        Some(channels[idx].clone())
    }

    /// Return suspended hosts.
    pub(crate) fn suspended(&self) -> Vec<Url> {
        self.registry
            .lock()
            .iter()
            .filter_map(
                |(url, state)| {
                    if matches!(state, HostState::Suspend) {
                        Some(url.clone())
                    } else {
                        None
                    }
                },
            )
            .collect()
    }

    /// Register a channel as connected.
    pub(crate) async fn register_channel(&self, channel: ChannelPtr) {
        let address = channel.address().clone();

        // Skip Tor-style inbound connections
        if channel.p2p().settings().read().await.inbound_addrs.contains(&address) {
            return
        }

        if let Err(e) = self.try_register(address, HostState::Connected(channel.clone())) {
            warn!(target: "net::hosts::register_channel", "[P2P] Error registering channel: {e:?}");
            return
        }

        self.channel_publisher.notify(Ok(channel)).await;
        *self.last_connection.lock() = Instant::now();
    }

    /// Insert addresses into the greylist after filtering.
    pub(crate) async fn insert(&self, color: HostColor, addrs: &[(Url, u64)]) {
        let filtered = self.filter_addresses(addrs).await;
        let mut count = 0;

        for (addr, last_seen) in filtered {
            if self.try_register(addr.clone(), HostState::Insert).is_err() {
                continue;
            }

            self.container.store_and_trim(color, addr.clone(), last_seen);
            let _ = self.unregister(&addr);
            count += 1;
        }

        if count > 0 {
            self.store_publisher.notify(count).await;
        }
    }

    /// Find a connectable address from the given hosts.
    pub(crate) async fn check_addrs(&self, hosts: Vec<(Url, u64)>) -> Option<(Url, u64)> {
        let settings = self.settings.read().await;
        let seeds = &settings.seeds;
        let external = self.external_addrs().await;

        for (host, last_seen) in hosts {
            if seeds.contains(&host) || external.contains(&host) {
                continue;
            }

            if self.try_register(host.clone(), HostState::Connect).is_ok() {
                return Some((host, last_seen))
            }
        }

        None
    }

    /// Move a host to the greylist.
    pub async fn greylist_host(&self, addr: &Url, last_seen: u64) -> Result<()> {
        self.move_host(addr, last_seen, HostColor::Grey).await?;
        self.unregister(addr)
    }

    /// Move a host to the whitelist.
    pub async fn whitelist_host(&self, addr: &Url, last_seen: u64) -> Result<()> {
        self.move_host(addr, last_seen, HostColor::White).await?;
        self.unregister(addr)
    }

    /// Move a host between lists (requires Move state).
    pub(crate) async fn move_host(
        &self,
        addr: &Url,
        last_seen: u64,
        dest: HostColor,
    ) -> Result<()> {
        self.try_register(addr.clone(), HostState::Move)?;

        if dest == HostColor::Black {
            if addr.host_str().is_none() {
                return Ok(())
            }
            if !self.settings.read().await.localnet && self.is_local_host(addr) {
                return Ok(())
            }
        }

        self.container.move_host(addr, last_seen, dest)
    }

    /// Get the last_seen for an address across all active lists.
    pub fn fetch_last_seen(&self, addr: &Url) -> Option<u64> {
        for color in [HostColor::Gold, HostColor::White, HostColor::Grey] {
            if let Some(ls) = self.container.get_last_seen(color, addr) {
                return Some(ls)
            }
        }
        None
    }

    /// Check if we have an existing connection to a host (any port).
    pub fn has_existing_connection(&self, url: &Url) -> bool {
        let host_str = match url.host_str() {
            Some(h) => h,
            None => return false,
        };
        self.container.contains_hostname(&[HostColor::Gold, HostColor::White], host_str)
    }

    async fn filter_addresses(&self, addrs: &[(Url, u64)]) -> Vec<(Url, u64)> {
        let settings = self.settings.read().await;
        let external_addrs = self.external_addrs().await;
        let mut result = vec![];

        'addr_loop: for (addr, last_seen) in addrs {
            // Validate format
            if addr.host_str().is_none() || addr.port().is_none() || addr.cannot_be_a_base() {
                continue;
            }

            // Skip configured seeds and peers
            if settings.seeds.contains(addr) || settings.peers.contains(addr) {
                continue;
            }

            // Skip blacklisted
            if self.container.contains(HostColor::Black, addr) || self.block_all_ports(addr) {
                continue;
            }

            let host = addr.host().unwrap();

            // Skip our own addresses
            if !settings.localnet {
                for ext in &external_addrs {
                    if host == ext.host().unwrap() {
                        continue 'addr_loop;
                    }
                }
            } else {
                for ext in &settings.external_addrs {
                    if addr.port() == ext.port() {
                        continue 'addr_loop;
                    }
                }
            }

            // Skip local addresses in production
            if !settings.localnet && self.is_local_host(addr) {
                continue;
            }

            // Validate transport-specific formats
            if !self.validate_transport(addr) {
                continue;
            }

            // Store unsupported transports on dark list
            if !settings.active_profiles.contains(&addr.scheme().to_string()) ||
                (!self.ipv6_available.load(Ordering::SeqCst) && self.is_ipv6(addr))
            {
                self.container.store_and_trim(HostColor::Dark, addr.clone(), *last_seen);
                self.container.refresh(HostColor::Dark, 86400);

                if !settings.mixed_profiles.contains(&addr.scheme().to_string()) {
                    continue;
                }
            }

            // Skip if already in active lists
            if self
                .container
                .contains_any(&[HostColor::Gold, HostColor::White, HostColor::Grey], addr)
            {
                continue;
            }

            result.push((addr.clone(), *last_seen));
        }

        result
    }

    fn validate_transport(&self, addr: &Url) -> bool {
        match addr.scheme() {
            "tcp" | "tcp+tls" => true,

            #[cfg(feature = "p2p-tor")]
            "tor" | "tor+tls" => {
                use std::str::FromStr;
                tor_hscrypto::pk::HsId::from_str(addr.host_str().unwrap()).is_ok()
            }

            #[cfg(feature = "p2p-nym")]
            "nym" | "nym+tls" => false, // Temp skip

            #[cfg(feature = "p2p-i2p")]
            "i2p" | "i2p+tls" => Self::is_i2p_host(addr.host_str().unwrap()),

            #[cfg(feature = "p2p-quic")]
            "quic" => true,

            _ => false,
        }
    }

    pub(crate) async fn import_blacklist(&self) -> Result<()> {
        let settings = self.settings.read().await;

        for (hostname, schemes, ports) in &settings.blacklist {
            let schemes =
                if schemes.is_empty() { vec!["tcp+tls".to_string()] } else { schemes.clone() };

            let ports = if ports.is_empty() { vec![0] } else { ports.clone() };

            for scheme in &schemes {
                for &port in &ports {
                    let url_string = if port == 0 {
                        format!("{scheme}://{hostname}")
                    } else {
                        format!("{scheme}://{hostname}:{port}")
                    };

                    if let Ok(url) = Url::parse(&url_string) {
                        self.container.store_and_trim(HostColor::Black, url, 0);
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if a host is blacklisted without a port (blocks all ports).
    pub(crate) fn block_all_ports(&self, url: &Url) -> bool {
        let host = match url.host() {
            Some(h) => h,
            None => return false,
        };

        self.container.lists.read()[HostColor::Black as usize]
            .iter()
            .any(|(u, _)| u.host() == Some(host.clone()) && u.port().is_none())
    }

    pub fn is_local_host(&self, url: &Url) -> bool {
        match url.host() {
            None => false,
            Some(Host::Ipv4(ip)) => !ip.unstable_is_global(),
            Some(Host::Ipv6(ip)) => !ip.unstable_is_global(),
            Some(Host::Domain(d)) => LOCAL_HOST_STRS.contains(&d),
        }
    }

    pub fn is_ipv6(&self, url: &Url) -> bool {
        matches!(url.host(), Some(Host::Ipv6(_)))
    }

    pub(crate) fn add_auto_addr(&self, addr: Ipv6Addr) {
        self.auto_self_addrs.lock().push(addr);
    }

    pub fn guess_auto_addr(&self) -> Option<Ipv6Addr> {
        let mut addrs = self.auto_self_addrs.lock();
        most_frequent_or_any(addrs.make_contiguous())
    }

    pub async fn external_addrs(&self) -> Vec<Url> {
        let mut addrs = self.settings.read().await.external_addrs.clone();
        for addr in &mut addrs {
            self.patch_port(addr);
            self.patch_auto_addr(addr);
        }
        addrs
    }

    fn patch_auto_addr(&self, addr: &mut Url) {
        if addr.scheme() != "tcp" && addr.scheme() != "tcp+tls" {
            return
        }

        if let Some(Host::Ipv6(ip)) = addr.host() {
            if ip.is_unspecified() {
                if let Some(auto) = self.guess_auto_addr() {
                    let _ = addr.set_ip_host(IpAddr::V6(auto));
                }
            }
        }
    }

    fn patch_port(&self, _addr: &mut Url) {
        // TODO: Lookup port from InboundSession when port is 0
    }

    #[cfg(feature = "p2p-i2p")]
    fn is_i2p_host(host: &str) -> bool {
        if !host.ends_with(".i2p") {
            return false
        }

        let name = host.trim_end_matches(".i2p");

        if name.ends_with(".b32") {
            let b32 = name.trim_end_matches(".b32");
            let decoded = crate::util::encoding::base32::decode(b32);
            return decoded.is_some() && decoded.unwrap().len() == 32
        }

        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    }

    pub async fn subscribe_store(&self) -> Subscription<usize> {
        self.store_publisher.clone().subscribe().await
    }

    pub async fn subscribe_channel(&self) -> Subscription<Result<ChannelPtr>> {
        self.channel_publisher.clone().subscribe().await
    }

    pub async fn subscribe_disconnect(&self) -> Subscription<Error> {
        self.disconnect_publisher.clone().subscribe().await
    }
}

// Copied from https://doc.rust-lang.org/stable/src/core/net/ip_addr.rs.html#839
trait UnstableFeatureIp {
    fn unstable_is_global(&self) -> bool;
    fn unstable_is_shared(&self) -> bool;
    fn unstable_is_benchmarking(&self) -> bool;
    fn unstable_is_reserved(&self) -> bool;
    fn unstable_is_documentation(&self) -> bool;
}

impl UnstableFeatureIp for Ipv4Addr {
    #[inline]
    fn unstable_is_global(&self) -> bool {
        !(self.octets()[0] == 0 // "This network"
            || self.is_private()
            || self.unstable_is_shared()
            || self.is_loopback()
            || self.is_link_local()
            // addresses reserved for future protocols (`192.0.0.0/24`)
            // .9 and .10 are documented as globally reachable so they're excluded
            || (
                self.octets()[0] == 192 && self.octets()[1] == 0 && self.octets()[2] == 0
                && self.octets()[3] != 9 && self.octets()[3] != 10
            )
            || self.unstable_is_documentation()
            || self.unstable_is_benchmarking()
            || self.unstable_is_reserved()
            || self.is_broadcast())
    }

    #[inline]
    fn unstable_is_shared(&self) -> bool {
        self.octets()[0] == 100 && (self.octets()[1] & 0b1100_0000 == 0b0100_0000)
    }

    #[inline]
    fn unstable_is_benchmarking(&self) -> bool {
        self.octets()[0] == 198 && (self.octets()[1] & 0xfe) == 18
    }

    #[inline]
    fn unstable_is_reserved(&self) -> bool {
        self.octets()[0] & 240 == 240 && !self.is_broadcast()
    }

    #[inline]
    fn unstable_is_documentation(&self) -> bool {
        matches!(self.octets(), [192, 0, 2, _] | [198, 51, 100, _] | [203, 0, 113, _])
    }
}

impl UnstableFeatureIp for Ipv6Addr {
    fn unstable_is_global(&self) -> bool {
        !(self.is_unspecified()
            || self.is_loopback()
            // IPv4-mapped Address (`::ffff:0:0/96`)
            || matches!(self.segments(), [0, 0, 0, 0, 0, 0xffff, _, _])
            // IPv4-IPv6 Translat. (`64:ff9b:1::/48`)
            || matches!(self.segments(), [0x64, 0xff9b, 1, _, _, _, _, _])
            // Discard-Only Address Block (`100::/64`)
            || matches!(self.segments(), [0x100, 0, 0, 0, _, _, _, _])
            // IETF Protocol Assignments (`2001::/23`)
            || (matches!(self.segments(), [0x2001, b, _, _, _, _, _, _] if b < 0x200)
                && !(
                    // Port Control Protocol Anycast (`2001:1::1`)
                    u128::from_be_bytes(self.octets()) == 0x2001_0001_0000_0000_0000_0000_0000_0001
                    // Traversal Using Relays around NAT Anycast (`2001:1::2`)
                    || u128::from_be_bytes(self.octets()) == 0x2001_0001_0000_0000_0000_0000_0000_0002
                    // AMT (`2001:3::/32`)
                    || matches!(self.segments(), [0x2001, 3, _, _, _, _, _, _])
                    // AS112-v6 (`2001:4:112::/48`)
                    || matches!(self.segments(), [0x2001, 4, 0x112, _, _, _, _, _])
                    // ORCHIDv2 (`2001:20::/28`)
                    // Drone Remote ID Protocol Entity Tags (DETs) Prefix (`2001:30::/28`)`
                    || matches!(self.segments(), [0x2001, b, _, _, _, _, _, _] if (0x20..=0x3F).contains(&b))
                ))
            // 6to4 (`2002::/16`) – it's not explicitly documented as globally reachable,
            // IANA says N/A.
            || matches!(self.segments(), [0x2002, _, _, _, _, _, _, _])
            || self.unstable_is_documentation()
            // Segment Routing (SRv6) SIDs (`5f00::/16`)
            || matches!(self.segments(), [0x5f00, ..])
            || self.is_unique_local()
            || self.is_unicast_link_local())
    }

    #[inline]
    fn unstable_is_shared(&self) -> bool {
        // Noop for ipv6
        false
    }

    #[inline]
    fn unstable_is_benchmarking(&self) -> bool {
        (self.segments()[0] == 0x2001) && (self.segments()[1] == 0x2) && (self.segments()[2] == 0)
    }

    #[inline]
    fn unstable_is_reserved(&self) -> bool {
        // Noop for ipv6
        false
    }

    #[inline]
    fn unstable_is_documentation(&self) -> bool {
        matches!(self.segments(), [0x2001, 0xdb8, ..] | [0x3fff, 0..=0x0fff, ..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hosts() -> HostsPtr {
        let settings = Settings::default();
        Hosts::new(Arc::new(AsyncRwLock::new(settings)))
    }

    #[test]
    fn test_is_local_host() {
        let hosts = make_hosts();

        let local = vec![
            "tcp://localhost:1234",
            "tcp://127.0.0.1:1234",
            "tcp+tls://[::1]:1234",
            "tcp://192.168.10.65:1234",
        ];

        for url in local {
            assert!(hosts.is_local_host(&Url::parse(url).unwrap()), "{url} should be local");
        }

        let remote = vec![
            "https://dyne.org:443",
            "tcp://77.168.10.65:2222",
            "tcp://[2345:0425:2CA1::5673:23b5]:1234",
        ];

        for url in remote {
            assert!(!hosts.is_local_host(&Url::parse(url).unwrap()), "{url} should be remote");
        }
    }

    #[test]
    fn test_container_operations() {
        let container = HostContainer::new();
        let url = Url::parse("tcp://test.com:1234").unwrap();
        let now = UNIX_EPOCH.elapsed().unwrap().as_secs();

        // Store and retrieve
        container.store(HostColor::Grey, url.clone(), now);
        assert!(container.contains(HostColor::Grey, &url));
        assert!(!container.contains(HostColor::White, &url));

        // Move atomically
        container.move_host(&url, now, HostColor::White).unwrap();
        assert!(!container.contains(HostColor::Grey, &url));
        assert!(container.contains(HostColor::White, &url));

        // Remove
        container.remove(HostColor::White, &url);
        assert!(!container.contains(HostColor::White, &url));
    }

    #[test]
    fn test_contains_any() {
        let container = HostContainer::new();
        let url = Url::parse("tcp://test.com:1234").unwrap();
        let now = UNIX_EPOCH.elapsed().unwrap().as_secs();

        container.store(HostColor::Gold, url.clone(), now);

        assert!(container.contains_any(&[HostColor::Grey, HostColor::Gold], &url));
        assert!(!container.contains_any(&[HostColor::Grey, HostColor::White], &url));
    }

    #[test]
    fn test_host_state_transitions() {
        let valid = [
            (HostState::Free(0), HostState::Insert),
            (HostState::Free(0), HostState::Refine),
            (HostState::Free(0), HostState::Connect),
            (HostState::Suspend, HostState::Refine),
            (HostState::Move, HostState::Suspend),
        ];

        for (from, to) in valid {
            assert!(from.try_transition(to).is_ok());
        }

        let invalid = [
            (HostState::Insert, HostState::Connect),
            (HostState::Refine, HostState::Insert),
            (HostState::Suspend, HostState::Connect),
        ];

        for (from, to) in invalid {
            assert!(from.try_transition(to).is_err());
        }
    }

    #[test]
    fn test_random_channel_empty() {
        let hosts = make_hosts();
        assert!(hosts.random_channel().is_none());
    }

    #[test]
    fn test_block_all_ports() {
        let hosts = make_hosts();

        let with_port = Url::parse("tcp+tls://example.com:333").unwrap();
        let without_port = Url::parse("tcp+tls://blocked.com").unwrap();

        hosts.container.store(HostColor::Black, with_port.clone(), 0);
        hosts.container.store(HostColor::Black, without_port.clone(), 0);

        let test_url = Url::parse("tcp+tls://blocked.com:9999").unwrap();
        assert!(hosts.block_all_ports(&test_url));

        let test_url2 = Url::parse("tcp+tls://example.com:9999").unwrap();
        assert!(!hosts.block_all_ports(&test_url2));
    }

    #[test]
    fn test_refresh() {
        let container = HostContainer::new();
        let old_time = 1720000000u64;
        let now = UNIX_EPOCH.elapsed().unwrap().as_secs();

        // Add old entries
        for i in 0..5 {
            let url = Url::parse(&format!("tcp://old{i}.com:123")).unwrap();
            container.store(HostColor::Dark, url, old_time);
        }

        // Add new entries
        for i in 0..5 {
            let url = Url::parse(&format!("tcp://new{i}.com:123")).unwrap();
            container.store(HostColor::Dark, url, now);
        }

        container.refresh(HostColor::Dark, 86400);

        let all = container.fetch_all(HostColor::Dark);
        assert_eq!(all.len(), 5);
        assert!(all.iter().all(|(_, ls)| *ls > old_time));
    }

    #[test]
    fn test_transport_mixing() {
        let hosts = HostContainer::mix_host(
            &Url::parse("tcp://dark.fi:28880").unwrap(),
            &["tor".to_string(), "tcp".to_string()],
            &["tcp".to_string()],
            &Url::parse("socks5://127.0.0.1:9050").ok(),
            &None,
        );

        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].scheme(), "tor");
    }

    #[test]
    fn test_prune_registry() {
        let hosts = make_hosts();
        let now = UNIX_EPOCH.elapsed().unwrap().as_secs();

        // Insert an entry that should be pruned (old Free)
        let old_url = Url::parse("tcp://old.example.com:123").unwrap();
        let old_age = now.saturating_sub(super::REGISTRY_PRUNE_AGE_SECS + 1000);
        hosts.registry.lock().insert(old_url.clone(), HostState::Free(old_age));

        // Insert an entry that should NOT be pruned (recent Free)
        let new_url = Url::parse("tcp://new.example.com:123").unwrap();
        hosts.registry.lock().insert(new_url.clone(), HostState::Free(now));

        // Insert an entry that should NOT be pruned (non-Free state)
        let active_url = Url::parse("tcp://active.example.com:123").unwrap();
        hosts.registry.lock().insert(active_url.clone(), HostState::Connect);

        assert_eq!(hosts.registry.lock().len(), 3);

        let pruned = hosts.prune_registry();
        assert_eq!(pruned, 1);

        let registry = hosts.registry.lock();
        assert_eq!(registry.len(), 2);
        assert!(!registry.contains_key(&old_url));
        assert!(registry.contains_key(&new_url));
        assert!(registry.contains_key(&active_url));
    }
}
