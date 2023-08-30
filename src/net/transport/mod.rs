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

/// TLS Upgrade Mechanism
pub(crate) mod tls;

#[cfg(feature = "p2p-tcp")]
/// TCP Transport
pub(crate) mod tcp;

#[cfg(feature = "p2p-tor")]
/// Tor transport
pub(crate) mod tor;

#[cfg(feature = "p2p-nym")]
/// Nym transport
pub(crate) mod nym;

#[cfg(feature = "p2p-unix")]
/// Unix socket transport
pub(crate) mod unix;

pub(crate) mod transport;

pub use transport::{Dialer, Listener, PtListener, PtStream};
