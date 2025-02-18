/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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

/// Common internal functions
mod common;

/// JSON-RPC primitives
pub mod jsonrpc;

/// Client-side JSON-RPC implementation
pub mod client;

/// Server-side JSON-RPC implementation
pub mod server;

/// Clock sync utility module
pub mod clock_sync;

/// Various `From` implementations
pub mod from_impl;

/// Provides optional `p2p.get_info()` method
pub mod p2p_method;

/// Json helper methods and types
pub mod util;

/// JSON-RPC settings
pub mod settings;