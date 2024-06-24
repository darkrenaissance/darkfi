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
use std::sync::Mutex;
use url::Url;

use super::{settings::SettingsPtr, ChannelPtr};
use crate::{
    system::{Publisher, PublisherPtr, Subscription},
    util::{
        file::{load_file, save_file},
        path::expand_path,
    },
    Error, Result,
};

/// The main interface for interacting with the hostlist. Contains the following:
///
/// `Hosts`: the main parent class that manages HostRegistry and HostContainer. It is also
///  responsible for filtering addresses before writing to the hostlist.
///
/// `HostRegistry`: A locked HashMap that maps peer addresses onto mutually exclusive
///  states (`HostState`). Prevents race conditions by dictating a strict flow of logically
///  acceptable states.
///
/// `HostContainer`: A wrapper for the hostlists. Each hostlist is represented by a `HostColor`,
///  which can be Grey, White, Gold or Black. Exposes a common interface for hostlist queries and
///  utilities.
///
/// `HostColor`: White hosts have been seen recently. Gold hosts we have been able to establish
///  a connection to. Grey hosts are recently received hosts that are periodically refreshed
///  using the greylist refinery. Black hosts are considered hostile and are strictly avoided
///  for the duration of the program. Dark hosts are hosts that do not match our transports, but
///  that we continue to share with other peers. They are otherwise ignored.
///
/// `HostState`: a set of mutually exclusive states that can be Insert, Refine, Connect, Suspend
///  or Connected. The state is `None` when the corresponding host has been removed from the
///  HostRegistry.

// An array containing all possible local host strings
// TODO: This could perhaps be more exhaustive?
pub const LOCAL_HOST_STRS: [&str; 2] = ["localhost", "localhost.localdomain"];
const WHITELIST_MAX_LEN: usize = 5000;
const GREYLIST_MAX_LEN: usize = 2000;
const DARKLIST_MAX_LEN: usize = 1000;

/// Atomic pointer to hosts object
pub type HostsPtr = Arc<Hosts>;

/// Keeps track of hosts and their current state. Prevents race conditions
/// where multiple threads are simultaneously trying to change the state of
/// a given host.
pub(in crate::net) type HostRegistry = RwLock<HashMap<Url, HostState>>;

/// HostState is a set of mutually exclusive states that can be Insert,
/// Refine, Move, Connect, Suspend or Connected. The state is `None` when the
/// corresponding host has been removed from the HostRegistry.
/// ```
///                +------+
///                | None |
///                +------+
///                   ^
///                   |
///                +------+      +---------+
///       +------> | move | ---> | suspend |
///       |        +------+      +---------+
///       |           |               |
///       |           |               v        +--------+
///  +---------+      |          +--------+    | insert |
///  | connect |      |          | refine |    +--------+
///  +---------+      |          +--------+        |
///       |           v               |            v
///       |     +-----------+         |         +------+
///       +---> | connected | <-------+-------> | None |
///             +-----------+                   +------+
///                   |
///                   v
///                +------+
///                | None |
///                +------+
///
/// ```
/* NOTE: Currently if a user loses connectivity, they will be deleted from
our hostlist by the refinery process and forgotten about until they regain
connectivity and share their external address with the p2p network again.

We may want to keep nodes with patchy connections in a `Red` list
and periodically try to connect to them in Outbound Session, rather
than sending them to the refinery (which will delete them if they are
offline) as we do using `Suspend`. The current design favors reliability
of connections but this may come at a risk for security since an attacker
is likely to have good uptime. We want to insure that users with patchy
connections or on mobile are still likely to be connected to.*/

#[derive(Clone, Debug)]
pub(in crate::net) enum HostState {
    /// Hosts that are currently being inserting into the hostlist.
    Insert,
    /// Hosts that are migrating from the greylist to the whitelist or being
    /// removed from the greylist, as defined in `refinery.rs`.
    Refine,
    /// Hosts that are being connected to in Outbound and Manual Session.
    Connect,
    /// Hosts that we have just failed to connect to. Marking a host as
    /// Suspend effectively sends this host to refinery, since Suspend->
    /// Refine is an acceptable state transition. Being marked as Suspend does
    /// not increase a host's probability of being refined, since the refinery
    /// selects its subjects randomly (with the caveat that we cannot refine
    /// nodes marked as Connect, Connected, Insert or Move). It does however
    /// mean this host cannot be connected to unless it passes through the
    /// refinery successfully.
    Suspend,
    /// Hosts that have been successfully connected to.
    Connected(ChannelPtr),

    /// Host that are moving between hostlists, implemented in
    /// store::move_host().
    Move,
}

impl HostState {
    // Try to change state to Insert. Only possible if we are not yet
    // tracking this host in the HostRegistry.
    fn try_insert(&self) -> Result<Self> {
        let start = self.to_string();
        let end = HostState::Insert.to_string();
        match self {
            HostState::Insert => Err(Error::HostStateBlocked(start, end)),
            HostState::Refine => Err(Error::HostStateBlocked(start, end)),
            HostState::Connect => Err(Error::HostStateBlocked(start, end)),
            HostState::Suspend => Err(Error::HostStateBlocked(start, end)),
            HostState::Connected(_) => Err(Error::HostStateBlocked(start, end)),
            HostState::Move => Err(Error::HostStateBlocked(start, end)),
        }
    }

    // Try to change state to Refine. Only possible if we are not yet
    // tracking this host in the HostRegistry or if the host is marked as
    // Suspend i.e. we have failed to connect to it.
    fn try_refine(&self) -> Result<Self> {
        let start = self.to_string();
        let end = HostState::Refine.to_string();
        match self {
            HostState::Insert => Err(Error::HostStateBlocked(start, end)),
            HostState::Refine => Err(Error::HostStateBlocked(start, end)),
            HostState::Connect => Err(Error::HostStateBlocked(start, end)),
            HostState::Suspend => Ok(HostState::Refine),
            HostState::Connected(_) => Err(Error::HostStateBlocked(start, end)),
            HostState::Move => Err(Error::HostStateBlocked(start, end)),
        }
    }

    // Try to change state to Connect. Only possible if we are not yet
    // tracking this host in the HostRegistry.
    fn try_connect(&self) -> Result<Self> {
        let start = self.to_string();
        let end = HostState::Connect.to_string();
        match self {
            HostState::Insert => Err(Error::HostStateBlocked(start, end)),
            HostState::Refine => Err(Error::HostStateBlocked(start, end)),
            HostState::Connect => Err(Error::HostStateBlocked(start, end)),
            HostState::Suspend => Err(Error::HostStateBlocked(start, end)),
            HostState::Connected(_) => Err(Error::HostStateBlocked(start, end)),
            HostState::Move => Err(Error::HostStateBlocked(start, end)),
        }
    }

    // Try to change state to Connected. Possible if this peer's state
    // is currently Connect or Refine, or Move. Refine is necessary since the
    // refinery process requires us to establish a connection to a peer.
    // Move is necessary due to the upgrade to Gold sequence in
    // `session::perform_handshake_protocols`.
    fn try_connected(&self, channel: ChannelPtr) -> Result<Self> {
        let start = self.to_string();
        let end = HostState::Connected(channel.clone()).to_string();
        match self {
            HostState::Insert => Err(Error::HostStateBlocked(start, end)),
            HostState::Refine => Ok(HostState::Connected(channel)),
            HostState::Connect => Ok(HostState::Connected(channel)),
            HostState::Suspend => Err(Error::HostStateBlocked(start, end)),
            HostState::Connected(_) => Err(Error::HostStateBlocked(start, end)),
            HostState::Move => Ok(HostState::Connected(channel)),
        }
    }

    // Try to change state to Move. Possibly if this host is currently
    // Connect i.e. it is being connected to, or if we are currently Connected
    // to this peer (due to host Downgrade sequence in `session::remove_sub_on_stop`)
    fn try_move(&self) -> Result<Self> {
        let start = self.to_string();
        let end = HostState::Move.to_string();
        match self {
            HostState::Insert => Err(Error::HostStateBlocked(start, end)),
            HostState::Refine => Err(Error::HostStateBlocked(start, end)),
            HostState::Connect => Ok(HostState::Move),
            HostState::Suspend => Err(Error::HostStateBlocked(start, end)),
            HostState::Connected(_) => Ok(HostState::Move),
            HostState::Move => Err(Error::HostStateBlocked(start, end)),
        }
    }

    // Try to change the state to Suspend. Only possible when we are
    // currently moving this host, since we suspend a host after failing
    // to connect to it in `outbound_session::try_connect` and then downgrading
    // in `hosts::move_host`.
    fn try_suspend(&self) -> Result<Self> {
        let start = self.to_string();
        let end = HostState::Suspend.to_string();
        match self {
            HostState::Insert => Err(Error::HostStateBlocked(start, end)),
            HostState::Refine => Err(Error::HostStateBlocked(start, end)),
            HostState::Connect => Err(Error::HostStateBlocked(start, end)),
            HostState::Suspend => Err(Error::HostStateBlocked(start, end)),
            HostState::Connected(_) => Err(Error::HostStateBlocked(start, end)),
            HostState::Move => Ok(HostState::Suspend),
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
    /// Intermediary nodes that are periodically probed and updated
    /// to White.
    Grey = 0,
    /// Recently seen hosts. Shared with other nodes.
    White = 1,
    /// Nodes to which we have already been able to establish a
    /// connection.
    Gold = 2,
    /// Hostile peers that can neither be connected to nor establish
    /// connections to us for the duration of the program.
    Black = 3,
    /// Peers that do not match our accepted transports. We are blind
    /// to these nodes (we do not use them) but we send them around
    /// the network anyway to ensure all transports are propagated.
    Dark = 4,
}

impl TryFrom<usize> for HostColor {
    type Error = Error;

    fn try_from(value: usize) -> Result<Self> {
        match value {
            0 => Ok(HostColor::Grey),
            1 => Ok(HostColor::White),
            2 => Ok(HostColor::Gold),
            3 => Ok(HostColor::Black),
            4 => Ok(HostColor::Dark),
            _ => Err(Error::InvalidHostColor),
        }
    }
}

/// A Container for managing Grey, White, Gold and Black hostlists. Exposes
/// a common interface for writing to and querying hostlists.
// TODO: Benchmark hostlist operations when the hostlist is at max size.
pub struct HostContainer {
    pub(in crate::net) hostlists: [RwLock<Vec<(Url, u64)>>; 5],
}

impl HostContainer {
    fn new() -> Self {
        let hostlists: [RwLock<Vec<(Url, u64)>>; 5] = [
            RwLock::new(Vec::new()),
            RwLock::new(Vec::new()),
            RwLock::new(Vec::new()),
            RwLock::new(Vec::new()),
            RwLock::new(Vec::new()),
        ];

        Self { hostlists }
    }

    /// Append host to a hostlist. Called when initalizing the hostlist in load_hosts().
    async fn store(&self, color: usize, addr: Url, last_seen: u64) {
        trace!(target: "net::hosts::store()", "[START] list={:?}",
        HostColor::try_from(color).unwrap());

        let mut list = self.hostlists[color].write().await;
        list.push((addr.clone(), last_seen));
        debug!(target: "net::hosts::store()", "Added [{}] to {:?} list",
               addr, HostColor::try_from(color).unwrap());

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

        if color == 4 && list.len() == DARKLIST_MAX_LEN {
            let last_entry = list.pop().unwrap();
            debug!(
                target: "net::hosts::store()",
                "Darklist reached max size. Removed {:?}", last_entry,
            );
        }

        // Sort the list by last_seen.
        list.sort_by_key(|entry| entry.1);
        list.reverse();

        trace!(target: "net::hosts::store()", "[END] list={:?}",
               HostColor::try_from(color).unwrap());
    }

    /// Stores an address on a hostlist or updates its last_seen field if
    /// we already have the address.
    async fn store_or_update(&self, color: HostColor, addr: Url, last_seen: u64) {
        trace!(target: "net::hosts::store_or_update()", "[START]");
        let color_code = color.clone() as usize;
        let mut list = self.hostlists[color_code].write().await;
        if let Some(position) = list.iter().position(|(u, _)| u == &addr) {
            list[position] = (addr.clone(), last_seen);
            debug!(target: "net::hosts::store_or_update()", "Updated [{}] entry on {:?} list",
                addr, color.clone());
        } else {
            list.push((addr.clone(), last_seen));
            debug!(target: "net::hosts::store_or_update()", "Added [{}] to {:?} list", addr, color);

            if color_code == 0 && list.len() == GREYLIST_MAX_LEN {
                let last_entry = list.pop().unwrap();
                debug!(
                    target: "net::hosts::store_or_update()",
                    "Greylist reached max size. Removed {:?}", last_entry,
                );
            }

            if color_code == 1 && list.len() == WHITELIST_MAX_LEN {
                let last_entry = list.pop().unwrap();
                debug!(
                    target: "net::hosts::store_or_update()",
                    "Whitelist reached max size. Removed {:?}", last_entry,
                );
            }

            if color_code == 4 && list.len() == DARKLIST_MAX_LEN {
                let last_entry = list.pop().unwrap();
                debug!(
                    target: "net::hosts::store_or_update()",
                    "Darklist reached max size. Removed {:?}", last_entry,
                );
            }

            // Sort the list by last_seen.
            list.sort_by_key(|entry| entry.1);
            list.reverse();
        }
        trace!(target: "net::hosts::store_or_update()", "[STOP]");
    }

    /// Update the last_seen field of a peer on a hostlist.
    pub async fn update_last_seen(
        &self,
        color: usize,
        addr: &Url,
        last_seen: u64,
        position: Option<usize>,
    ) {
        trace!(target: "net::hosts::update_last_seen()", "[START] list={:?}",
        HostColor::try_from(color).unwrap());

        let i = match position {
            Some(i) => i,
            None => self.get_index_at_addr(color, addr.clone()).await.unwrap(),
        };

        let mut list = self.hostlists[color].write().await;
        list[i] = (addr.clone(), last_seen);
        list.sort_by_key(|entry| entry.1);
        list.reverse();

        trace!(target: "net::hosts::update_last_seen()", "[END] list={:?}",
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

    /// Fetch addresses that match the provided transports or acceptable
    /// mixed transports.  Will return an empty Vector if no such addresses
    /// were found.
    pub(in crate::net) async fn fetch(
        &self,
        color: HostColor,
        transports: &[String],
        transport_mixing: bool,
    ) -> Vec<(Url, u64)> {
        trace!(target: "net::hosts::fetch_addrs()", "[START] {:?}", color);
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

        trace!(target: "net::hosts::fetch_addrs()", "Grabbed hosts, length: {}", hosts.len());

        hosts
    }

    /// Get up to limit peers that match the given transport schemes from
    /// a hostlist.  If limit was not provided, return all matching peers.
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
                           "Found matching addr on list={:?}, returning {} addresses",
                           HostColor::try_from(color).unwrap(), ret.len());
                    return ret
                }
            }
        }

        if ret.is_empty() {
            debug!(target: "net::hosts::fetch_with_schemes()",
                   "No matching schemes found on list={:?}!", HostColor::try_from(color).unwrap())
        }

        ret
    }

    /// Get up to limit peers that don't match the given transport schemes
    /// from a hostlist.  If limit was not provided, return all matching
    /// peers.
    async fn fetch_excluding_schemes(
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
            debug!(target: "net::hosts::fetch_excluding_schemes()", "No such schemes found!");
        }

        ret
    }

    /// Get a random peer from a hostlist that matches the given transport
    /// schemes.
    pub(in crate::net) async fn fetch_random_with_schemes(
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
    pub(in crate::net) async fn fetch_n_random(&self, color: HostColor, n: u32) -> Vec<(Url, u64)> {
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
            debug!(target: "net::hosts::fetch_n_random()", "No entries found!");
            return hosts
        }

        // Grab random ones
        let urls = hosts.iter().choose_multiple(&mut OsRng, n.min(hosts.len()));
        urls.iter().map(|&url| url.clone()).collect()
    }

    /// Get up to n random peers that match the given transport schemes.
    pub(in crate::net) async fn fetch_n_random_with_schemes(
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

    /// Get up to n random peers that don't match the given transport schemes
    /// from a hostlist.
    pub(in crate::net) async fn fetch_n_random_excluding_schemes(
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

    /// Remove an entry from a hostlist if it exists.
    async fn remove_if_exists(&self, color: HostColor, addr: &Url) {
        let color_code = color.clone() as usize;
        let mut list = self.hostlists[color_code].write().await;
        if let Some(position) = list.iter().position(|(u, _)| u == addr) {
            debug!(target: "net::hosts::remove_if_exists()", "Removing addr={} list={:?}", addr, color);
            list.remove(position);
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
    async fn get_index_at_addr(&self, color: usize, addr: Url) -> Option<usize> {
        self.hostlists[color].read().await.iter().position(|a| a.0 == addr)
    }

    /// Get the last_seen field for a given entry on a hostlist.
    pub async fn get_last_seen(&self, color: usize, addr: &Url) -> Option<u64> {
        self.hostlists[color]
            .read()
            .await
            .iter()
            .find(|(url, _)| url == addr)
            .map(|(_, last_seen)| *last_seen)
    }

    /// Load the hostlists from a file.
    pub(in crate::net) async fn load_all(&self, path: &str) -> Result<()> {
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
                "gold" => {
                    self.store(HostColor::Gold as usize, url, last_seen).await;
                }
                "white" => {
                    self.store(HostColor::White as usize, url, last_seen).await;
                }
                "grey" => {
                    self.store(HostColor::Grey as usize, url, last_seen).await;
                }
                "dark" => {
                    self.store(HostColor::Dark as usize, url, last_seen).await;
                }
                _ => {
                    debug!(target: "net::hosts::load_hosts()", "Malformed list name...");
                }
            }
        }

        Ok(())
    }

    /// Save the hostlist to a file.
    pub(in crate::net) async fn save_all(&self, path: &str) -> Result<()> {
        let path = expand_path(path)?;

        let mut tsv = String::new();
        let mut hostlist: HashMap<String, Vec<(Url, u64)>> = HashMap::new();

        hostlist.insert("dark".to_string(), self.fetch_all(HostColor::Dark).await);
        hostlist.insert("grey".to_string(), self.fetch_all(HostColor::Grey).await);
        hostlist.insert("white".to_string(), self.fetch_all(HostColor::White).await);
        hostlist.insert("gold".to_string(), self.fetch_all(HostColor::Gold).await);

        for (name, list) in hostlist {
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

/// Main parent class for the management and manipulation of
/// hostlists. Keeps track of hosts and their current state via the
/// HostRegistry, and stores hostlists and associated methods in the
/// HostContainer. Also operates two publishers to notify other parts
/// of the code base when new channels have been created or new hosts
/// have been added to the hostlist.
pub struct Hosts {
    /// A registry that tracks hosts and their current state.
    registry: HostRegistry,

    /// Hostlists and associated methods.
    pub container: HostContainer,

    /// Publisher listening for store updates
    store_publisher: PublisherPtr<usize>,

    /// Publisher for notifications of new channels
    pub(in crate::net) channel_publisher: PublisherPtr<Result<ChannelPtr>>,

    /// Keeps track of the last time a connection was made.
    pub(in crate::net) last_connection: RwLock<Instant>,

    /// Marker for IPv6 availability
    pub(in crate::net) ipv6_available: Mutex<bool>,

    /// Pointer to configured P2P settings
    settings: SettingsPtr,
}

impl Hosts {
    /// Create a new hosts list
    pub(in crate::net) fn new(settings: SettingsPtr) -> HostsPtr {
        Arc::new(Self {
            registry: RwLock::new(HashMap::new()),
            container: HostContainer::new(),
            store_publisher: Publisher::new(),
            channel_publisher: Publisher::new(),
            last_connection: RwLock::new(Instant::now()),
            ipv6_available: Mutex::new(true),
            settings,
        })
    }

    /// Safely insert into the HostContainer. Filters the addresses first before storing and
    /// notifies the publisher. Must be called when first receiving greylist addresses.
    pub(in crate::net) async fn insert(&self, color: HostColor, addrs: &[(Url, u64)]) {
        trace!(target: "net::hosts:insert()", "[START]");

        // First filter these address to ensure this peer doesn't exist in our black, gold or
        // whitelist and apply transport filtering. If we don't support this transport,
        // store the peer on our dark list to broadcast to other nodes.
        let filtered_addrs = self.filter_addresses(self.settings.clone(), addrs).await;
        let mut addrs_len = 0;

        if filtered_addrs.is_empty() {
            debug!(target: "net::hosts::insert()", "Filtered out all addresses");
        }

        // Then ensure we aren't currently trying to add this peer to the hostlist.
        for (i, (addr, last_seen)) in filtered_addrs.iter().enumerate() {
            if let Err(e) = self.try_register(addr.clone(), HostState::Insert).await {
                debug!(target: "net::hosts::store_or_update", "Cannot insert addr={}, err={}",
                       addr.clone(), e);

                continue
            }

            addrs_len += i + 1;
            self.container.store_or_update(color.clone(), addr.clone(), *last_seen).await;

            // Free up this peer for usage by other parts of the code base.
            // This is a safe since the hostlist modification is now complete.
            self.unregister(addr).await;
        }

        self.store_publisher.notify(addrs_len).await;
        trace!(target: "net::hosts:insert()", "[END]");
    }

    /// Check whether a peer is available to be refined currently. Returns true
    /// if available, false otherwise.
    pub async fn refinable(&self, addr: Url) -> bool {
        self.try_register(addr.clone(), HostState::Refine).await.is_ok()
    }

    /// Try to update the registry. If the host already exists, try to update its state.
    /// Otherwise add the host to the registry along with its state.
    pub(in crate::net) async fn try_register(
        &self,
        addr: Url,
        new_state: HostState,
    ) -> Result<HostState> {
        let mut registry = self.registry.write().await;

        trace!(target: "net::hosts::try_update_registry()", "Try register addr={}, state={}",
               addr, &new_state);

        if registry.contains_key(&addr) {
            let current_state = registry.get(&addr).unwrap().clone();

            let result: Result<HostState> = match new_state {
                HostState::Insert => current_state.try_insert(),
                HostState::Refine => current_state.try_refine(),
                HostState::Connect => current_state.try_connect(),
                HostState::Suspend => current_state.try_suspend(),
                HostState::Connected(c) => current_state.try_connected(c),
                HostState::Move => current_state.try_move(),
            };

            if let Ok(state) = &result {
                registry.insert(addr.clone(), state.clone());
            }

            trace!(target: "net::hosts::try_update_registry()", "Returning result {:?}", result);

            result
        } else {
            // We don't know this peer. We can safely update the state.
            debug!(target: "net::hosts::try_update_registry()", "Inserting addr={}, state={}",
                   addr, &new_state);

            registry.insert(addr.clone(), new_state.clone());

            Ok(new_state)
        }
    }

    // Loop through hosts selected by Outbound Session and see if any of them are
    // free to connect to.
    pub(in crate::net) async fn check_addrs(&self, hosts: Vec<(Url, u64)>) -> Option<(Url, u64)> {
        trace!(target: "net::hosts::check_addrs()", "[START]");
        for (host, last_seen) in hosts {
            // Print a warning if we are trying to connect to a seed node in
            // Outbound session. This shouldn't happen as we reject configured
            // seed nodes from entering our hostlist in filter_addrs().
            if self.settings.seeds.contains(&host) {
                warn!(target: "net::hosts::check_addrs",
                      "Seed addr={} has entered the hostlist! Skipping",
                      host.clone());
                continue
            }

            if let Err(e) = self.try_register(host.clone(), HostState::Connect).await {
                trace!(target: "net::hosts::check_addrs", "Skipping addr={}, err={}",
                       host.clone(), e);
                continue
            }

            debug!(target: "net::hosts::check_addrs()", "Found valid host {}", host);
            return Some((host.clone(), last_seen))
        }

        None
    }

    /// Remove a host from the HostRegistry. Must be called after move(), when the refinery
    /// process fails, or when a channel stops. Prevents hosts from getting trapped in the
    /// HostState logical machinery.
    ///
    /// Misuse of this call is dangerous since it frees up the peer to be used by
    /// the refinery or outbound connect loop, and may result in invalid states. It should
    /// only be called when it is completely safe to do so.
    pub(in crate::net) async fn unregister(&self, addr: &Url) {
        self.registry.write().await.remove(addr);
        debug!(target: "net::hosts::unregister()", "Removed {} from HostRegistry", addr);
    }

    /// Returns the list of connected channels.
    pub async fn channels(&self) -> Vec<ChannelPtr> {
        let registry = self.registry.read().await;
        let mut channels = Vec::new();

        for (_, state) in registry.iter() {
            if let HostState::Connected(c) = state {
                channels.push(c.clone());
            }
        }
        channels
    }

    /// Returns the list of suspended channels.
    pub(in crate::net) async fn suspended(&self) -> Vec<Url> {
        let registry = self.registry.read().await;
        let mut addrs = Vec::new();

        for (url, state) in registry.iter() {
            if let HostState::Suspend = state {
                addrs.push(url.clone());
            }
        }
        addrs
    }

    /// Retrieve a random connected channel
    pub async fn random_channel(&self) -> ChannelPtr {
        let channels = self.channels().await;
        let position = rand::thread_rng().gen_range(0..channels.len());
        channels[position].clone()
    }

    /// Add a channel to the set of connected channels
    pub(in crate::net) async fn register_channel(&self, channel: ChannelPtr) {
        let address = channel.address().clone();

        // This will panic if we are already connected to this peer, this peer
        // is suspended, or this peer is currently being inserted into the hostlist.
        // None of these scenarios should ever happen.
        self.try_register(address.clone(), HostState::Connected(channel.clone())).await.unwrap();

        // Notify that channel processing was successful
        self.channel_publisher.notify(Ok(channel.clone())).await;

        let mut last_online = self.last_connection.write().await;
        *last_online = Instant::now();
    }

    pub async fn subscribe_store(&self) -> Subscription<usize> {
        self.store_publisher.clone().subscribe().await
    }

    pub async fn subscribe_channel(&self) -> Subscription<Result<ChannelPtr>> {
        self.channel_publisher.clone().subscribe().await
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

    /// Check whether a URL is IPV6
    pub async fn is_ipv6(&self, url: Url) -> bool {
        // Reject Urls without host strings.
        if url.host_str().is_none() {
            return false
        }

        // We do this hack in order to parse IPs properly.
        // https://github.com/whatwg/url/issues/749
        let addr = Url::parse(&url.as_str().replace(url.scheme(), "http")).unwrap();
        if let url::Host::Ipv6(_) = addr.host().unwrap() {
            return true
        }
        false
    }

    /// Import blacklisted peers specified in the config file.
    pub(in crate::net) async fn import_blacklist(&self) -> Result<()> {
        for (mut host, ports) in self.settings.blacklist.clone() {
            // If the ports are empty, simply store the host_str. We will use this to
            // blacklist all ports of a given peer in `block_all_ports()`.
            if ports.is_empty() {
                self.container.store(HostColor::Black as usize, host.clone(), 0).await;
            }
            // Otherwise, store all the specified ports.
            else {
                for port in ports {
                    host.set_port(Some(port))?;
                    self.container.store(HostColor::Black as usize, host.clone(), 0).await;
                }
            }
        }
        Ok(())
    }

    /// If we have the Host of the Url in the hostlist, and there are no ports stored,
    /// we should block all ports of this peer.
    pub(in crate::net) async fn block_all_ports(&self, addr: String) -> bool {
        self.container.hostlists[HostColor::Black as usize]
            .read()
            .await
            .iter()
            .any(|(u, _t)| u.host_str().unwrap() == addr && u.port().is_none())
    }

    /// Filter given addresses based on certain rulesets and validity. Strictly called only on
    /// the first time learning of new peers.
    async fn filter_addresses(
        &self,
        settings: SettingsPtr,
        addrs: &[(Url, u64)],
    ) -> Vec<(Url, u64)> {
        debug!(target: "net::hosts::filter_addresses()", "Filtering addrs: {:?}", addrs);
        let mut ret = vec![];
        let localnet = self.settings.localnet;
        let ipv6_available: bool = { *self.ipv6_available.lock().unwrap() };

        'addr_loop: for (addr_, last_seen) in addrs {
            // Validate that the format is `scheme://host_str:port`
            if addr_.host_str().is_none() ||
                addr_.port().is_none() ||
                addr_.cannot_be_a_base() ||
                addr_.path_segments().is_some()
            {
                debug!(target: "net::hosts::filter_addresses()",
                       "[{}] has invalid addr format. Skipping", addr_);
                continue
            }

            // Configured seeds should never enter the hostlist.
            if self.settings.seeds.contains(addr_) {
                debug!(target: "net::hosts::filter_addresses()",
                       "[{}] is a configured seed. Skipping", addr_);
                continue
            }

            // Blacklist peers should never enter the hostlist.
            if self.container.contains(HostColor::Black as usize, addr_).await ||
                self.block_all_ports(addr_.host_str().unwrap().to_string()).await
            {
                warn!(target: "net::hosts::filter_addresses()",
                      "[{}] is blacklisted", addr_);
                continue
            }

            let host_str = addr_.host_str().unwrap();

            if !localnet {
                // Our own external addresses should never enter the hosts set.
                for ext in &settings.external_addrs {
                    if host_str == ext.host_str().unwrap() {
                        debug!(target: "net::hosts::filter_addresses()",
                               "[{}] is our own external addr. Skipping", addr_);
                        continue 'addr_loop
                    }
                }
            } else {
                // On localnet, make sure ours ports don't enter the host set.
                for ext in &settings.external_addrs {
                    if addr_.port() == ext.port() {
                        debug!(target: "net::hosts::filter_addresses()",
                               "[{}] is our own localnet port. Skipping", addr_);
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
                debug!(target: "net::hosts::filter_addresses()",
                       "[{}] Filtering non-global ranges", addr_);
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

            // Store this peer on Dark list if we do not support this transport
            // or if this peer is IPV6 and we do not support IPV6.
            // We will personally ignore this peer but still send it to others in
            // Protocol Addr to ensure all transports get propagated.
            if !settings.allowed_transports.contains(&addr_.scheme().to_string()) ||
                (!ipv6_available && self.is_ipv6(addr_.clone()).await)
            {
                self.container.store_or_update(HostColor::Dark, addr_.clone(), *last_seen).await;

                continue
            }

            // Reject this peer if it's already stored on the Gold, White or Grey list.
            //
            // We do this last since it is the most expensive operation.
            if self.container.contains(HostColor::Gold as usize, addr_).await ||
                self.container.contains(HostColor::White as usize, addr_).await ||
                self.container.contains(HostColor::Grey as usize, addr_).await
            {
                debug!(target: "net::hosts::filter_addresses()", "[{}] exists! Skipping", addr_);
                continue
            }

            ret.push((addr_.clone(), *last_seen));
        }

        ret
    }

    /// Method to fetch the last_seen field for a give address when we do
    /// not know what hostlist it is on.
    pub async fn fetch_last_seen(&self, addr: &Url) -> Option<u64> {
        if self.container.contains(HostColor::Gold as usize, addr).await {
            self.container.get_last_seen(HostColor::Gold as usize, addr).await
        } else if self.container.contains(HostColor::White as usize, addr).await {
            self.container.get_last_seen(HostColor::White as usize, addr).await
        } else if self.container.contains(HostColor::Grey as usize, addr).await {
            self.container.get_last_seen(HostColor::Grey as usize, addr).await
        } else {
            None
        }
    }

    /// Downgrade host to Greylist, remove from Gold or White list.
    pub async fn greylist_host(&self, addr: &Url, last_seen: u64) -> Result<()> {
        debug!(target: "net::hosts:greylist_host()", "Downgrading addr={}", addr);
        self.move_host(addr, last_seen, HostColor::Grey).await?;

        // Free up this addr for future operations.
        self.unregister(addr).await;

        Ok(())
    }

    /// A single atomic function for moving hosts between hostlists. Called on the following occasions:
    ///
    /// * When we cannot connect to a peer: move to grey, remove from white and gold.
    /// * When a peer disconnects from us: move to grey, remove from white and gold.
    /// * When the refinery passes successfully: move to white, remove from greylist.
    /// * When we connect to a peer, move to gold, remove from white or grey.
    /// * When we add a peer to the black list: move to black, remove from all other lists.
    pub(in crate::net) async fn move_host(
        &self,
        addr: &Url,
        last_seen: u64,
        destination: HostColor,
    ) -> Result<()> {
        debug!(target: "net::hosts::move_host()", "Trying to move addr={} destination={:?}",
               addr, destination);

        // This should never panic. Failure indicates a misuse of the HostState API.
        self.try_register(addr.clone(), HostState::Move).await.unwrap();

        match destination {
            // Downgrade to grey. Remove from white and gold.
            HostColor::Grey => {
                self.container.remove_if_exists(HostColor::Gold, addr).await;
                self.container.remove_if_exists(HostColor::White, addr).await;
                self.container.store_or_update(HostColor::Grey, addr.clone(), last_seen).await;
            }

            // Remove from Greylist, add to Whitelist. Called by the Refinery.
            HostColor::White => {
                self.container.remove_if_exists(HostColor::Grey, addr).await;
                self.container.store_or_update(HostColor::White, addr.clone(), last_seen).await;
            }

            // Upgrade to gold. Remove from white or grey.
            HostColor::Gold => {
                self.container.remove_if_exists(HostColor::Grey, addr).await;
                self.container.remove_if_exists(HostColor::White, addr).await;
                self.container.store_or_update(HostColor::Gold, addr.clone(), last_seen).await;
            }

            // Move to black. Remove from all other lists.
            HostColor::Black => {
                // We ignore UNIX sockets here so we will just work
                // with stuff that has host_str().
                if addr.host_str().is_some() {
                    // Localhost connections should never enter the blacklist
                    // This however allows any Tor and Nym connections.
                    if self.is_local_host(addr.clone()).await {
                        return Ok(());
                    }

                    self.container.remove_if_exists(HostColor::Grey, addr).await;
                    self.container.remove_if_exists(HostColor::White, addr).await;
                    self.container.remove_if_exists(HostColor::Gold, addr).await;
                    self.container.store_or_update(HostColor::Black, addr.clone(), last_seen).await;
                }
            }

            HostColor::Dark => return Err(Error::InvalidHostColor),
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use super::{super::settings::Settings, *};
    use crate::system::sleep;

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
    fn test_is_ipv6() {
        smol::block_on(async {
            let settings = Settings { ..Default::default() };
            let hosts = Hosts::new(Arc::new(settings.clone()));

            let ipv6_hosts: Vec<Url> = vec![
                Url::parse("tcp+tls://[::1]").unwrap(),
                Url::parse("tcp://[2001:0000:130F:0000:0000:09C0:876A:130B]").unwrap(),
                Url::parse("tcp://[2345:0425:2CA1:0000:0000:0567:5673:23b5]").unwrap(),
            ];

            let ipv4_hosts: Vec<Url> = vec![
                Url::parse("tcp://192.168.10.65").unwrap(),
                Url::parse("https://dyne.org").unwrap(),
                Url::parse("tcp+tls://agorism.xyz").unwrap(),
            ];

            for host in ipv6_hosts {
                assert!(hosts.is_ipv6(host).await)
            }

            for host in ipv4_hosts {
                assert!(!hosts.is_ipv6(host).await)
            }
        });
    }

    #[test]
    fn test_block_all_ports() {
        smol::block_on(async {
            let settings = Settings { ..Default::default() };

            let hosts = Hosts::new(Arc::new(settings.clone()));
            let blacklist1 = Url::parse("tcp+tls://nietzsche.king:333").unwrap();
            let blacklist2 = Url::parse("tcp+tls://agorism.xyz").unwrap();

            hosts.container.store(HostColor::Black as usize, blacklist1.clone(), 0).await;
            hosts.container.store(HostColor::Black as usize, blacklist2.clone(), 0).await;

            assert!(hosts.block_all_ports(blacklist2.host_str().unwrap().to_string()).await);
            assert!(!hosts.block_all_ports(blacklist1.host_str().unwrap().to_string()).await);
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
}
