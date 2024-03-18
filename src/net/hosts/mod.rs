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

/// Periodically probes entries in the greylist.
///
/// Randomly selects a greylist entry and tries to
/// establish a local connection to it using the method probe_node(), which creates
/// a channel and does a version exchange using `perform_handshake_protocols()`.
///
/// If successful, the entry is removed from the greylist and added to the whitelist
/// with an updated last_seen timestamp. If non-successful, the entry is removed from
/// the greylist.
///
/// The method `probe_node()` is also used by `ProtocolSeed` and `ProtocolAddress`.
/// We try to establish local connections to our own external addresses using
/// `probe_node()` to ensure the address is valid before propagating in `ProtocolSeed`
/// and `ProtocolAddress`.
pub mod refinery;

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
///  using the greylist refinery. Black hosts are considerede hostile and are strictly avoided
///  for the duration of the program.
///
/// `HostState`: a set of mutually exclusive states that can be Insert, Refine, Connect, Suspend
///  or Connected. The state is `None` when the corresponding host has been removed from the
///  HostRegistry.
pub mod store;
