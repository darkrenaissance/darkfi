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

pub mod fixed_bases;
pub mod sinsemilla;
pub mod util;

pub use fixed_bases::{NullifierK, OrchardFixedBases, OrchardFixedBasesFull, ValueCommitV, H};

/// Domain prefix used for Schnorr signatures, with `hash_to_scalar`.
pub const DRK_SCHNORR_DOMAIN: &[u8] = b"DarkFi:Schnorr";

/// Domain prefix used for block hashes, with `hash_to_curve`.
pub const BLOCK_HASH_DOMAIN: &str = "DarkFi:Block";

pub const MERKLE_DEPTH_ORCHARD: usize = 32;

pub const MERKLE_DEPTH: u8 = MERKLE_DEPTH_ORCHARD as u8;

#[allow(dead_code)]
/// $\ell^\mathsf{Orchard}_\mathsf{base}$
pub(crate) const L_ORCHARD_BASE: usize = 255;

/// $\ell^\mathsf{Orchard}_\mathsf{scalar}$
pub(crate) const L_ORCHARD_SCALAR: usize = 255;

/// $\ell_\mathsf{value}$
pub(crate) const L_VALUE: usize = 64;

/// main-net wallet import format prefix
pub const MAINNET_ADDRS_PREFIX: [u8; 1] = [0x80];

/// test-net wallet import format prefix
pub const TESTNET_ADDRS_PREFIX: [u8; 1] = [0x01];

/// WIF checksum length
pub const WIF_CHECKSUM_LEN: usize = 4;
