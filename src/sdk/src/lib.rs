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

pub use bridgetree;
pub use num_bigint;
pub use num_traits;
pub use pasta_curves as pasta;

/// Blockchain structures
pub mod blockchain;

/// Database functions
pub mod db;

/// Contract deployment utilities
pub mod deploy;

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

/// Transaction structure
pub mod tx;
pub use tx::ContractCall;

/// Utility functions
pub mod util;

/// DarkTree structures
pub mod dark_tree;

/// Creates a hex formatted string of the data
#[inline]
pub fn hex_from_iter<I: Iterator<Item = u8>>(iter: I) -> String {
    let mut repr = String::new();
    for b in iter {
        repr += &format!("{:02x}", b);
    }
    repr
}

pub trait AsHex {
    fn hex(&self) -> String;
}

impl<T: AsRef<[u8]>> AsHex for T {
    /// Creates a hex formatted string of the data (big endian)
    fn hex(&self) -> String {
        hex_from_iter(self.as_ref().iter().cloned())
    }
}
