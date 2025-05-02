/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 * Copyright (C) 2021 MONOLOG (Taeho Francis Lim and Jongwhan Lee) MIT License
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

/// Size of fixed length byte-array from a `Hasher`.
/// Equivalent to `key` length of the tree.
pub const HASH_LEN: usize = 32;

/// A type representing length of `Bits`
pub type BitsLen = u16;

/// Type indicating fixed length byte-array.
pub type Hash = [u8; HASH_LEN];

/// Type representing a Merkle proof
pub type Proof = Vec<(bool, Vec<u8>)>;

/// The key to be used to restore the latest `root`
pub const ROOT_KEY: &Hash = b"_______monotree::headroot_______";

use std::sync::LazyLock;
pub static EMPTY_HASH: LazyLock<Hash> = LazyLock::new(|| *blake3::hash(&[]).as_bytes());

pub mod bits;

pub mod node;

pub mod tree;
pub use tree::Monotree;

pub mod utils;

#[cfg(test)]
mod tests;
