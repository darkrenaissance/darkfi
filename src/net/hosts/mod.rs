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

/// The main interface for interacting with the hostlist.
///
/// The hostlist is stored in three sections: white, grey, and anchorlists.
/// The _whitelist_ contains hosts that have been seen recently.
/// The _anchorlist_ contains hosts that we have been able to establish a connection to.
/// The _greylist_ is an intermediary host list of recently received hosts that is
/// periodically refreshed using the greylist refinery.
///
/// `store` contains various methods for reading from, quering and writing to the hostlists.
/// It is also responsible for filtering addresses and ensuring channel transport validity.
pub(super) mod store;
