/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2022 Dyne.org foundation
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

use lazy_static::lazy_static;

use crate::util::time::Timestamp;

lazy_static! {
    /// Genesis hash for the mainnet chain
    pub static ref MAINNET_GENESIS_HASH_BYTES: blake3::Hash = blake3::hash(b"darkfi_mainnet");

    /// Genesis timestamp for the mainnet chain
    pub static ref MAINNET_GENESIS_TIMESTAMP: Timestamp = Timestamp(1650887115);

    /// Genesis hash for the testnet chain
    pub static ref TESTNET_GENESIS_HASH_BYTES: blake3::Hash = blake3::hash(b"darkfi_testnet");

    /// Genesis timestamp for the testnet chain
    pub static ref TESTNET_GENESIS_TIMESTAMP: Timestamp = Timestamp(1650887115);
}

/// Block version number
pub const BLOCK_VERSION: u8 = 1;

/// Block magic bytes
pub const BLOCK_MAGIC_BYTES: [u8; 4] = [0x11, 0x6d, 0x75, 0x1f];

/// Block info magic bytes
pub const BLOCK_INFO_MAGIC_BYTES: [u8; 4] = [0x90, 0x44, 0xf1, 0xf6];

/// Number of slots in one epoch
pub const EPOCH_LENGTH: usize = 10;

/// Slot time in seconds
pub const SLOT_TIME: u64 = 20;

/// Block leader reward
pub const REWARD: u64 = 420;

/// Leader proofs k for zk proof rows (rows=2^k)
pub const LEADER_PROOF_K: u32 = 13;

// TODO: Describe these constants
pub const RADIX_BITS: usize = 76;
pub const P: &str = "28948022309329048855892746252171976963363056481941560715954676764349967630337";
pub const LOTTERY_HEAD_START: u64 = 1;
pub const PRF_NULLIFIER_PREFIX: u64 = 0;
