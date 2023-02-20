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

pub use incrementalmerkletree;
pub use pasta_curves as pasta;

/// Database functions
pub mod db;

/// Entrypoint used for the wasm binaries
pub mod entrypoint;

/// Error handling
pub mod error;

/// Logging infrastructure
pub mod log;

/// Crypto-related definitions
pub mod crypto;

/// Merkle
pub mod merkle;
pub use merkle::merkle_add;

/// Transaction structure
pub mod tx;
pub use tx::ContractCall;
