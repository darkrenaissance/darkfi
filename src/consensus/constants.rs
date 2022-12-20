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

use crate::{consensus::Float10, util::time::Timestamp};

lazy_static! {
    /// Genesis hash for the mainnet chain
    pub static ref MAINNET_GENESIS_HASH_BYTES: blake3::Hash = blake3::hash(b"darkfi_mainnet");

    // NOTE: On initial network bootstrap, genesis timestamp should be equal to boostrap timestamp.
    // On network restart only change bootstrap timestamp to schedule when nodes become active.
    /// Genesis timestamp for the mainnet chain
    pub static ref MAINNET_GENESIS_TIMESTAMP: Timestamp = Timestamp(1650887115);

    /// Bootstrap timestamp for the mainnet chain
    pub static ref MAINNET_BOOTSTRAP_TIMESTAMP: Timestamp = Timestamp(1650887115);

    /// Genesis hash for the testnet chain
    pub static ref TESTNET_GENESIS_HASH_BYTES: blake3::Hash = blake3::hash(b"darkfi_testnet");

    /// Genesis timestamp for the testnet chain
    pub static ref TESTNET_GENESIS_TIMESTAMP: Timestamp = Timestamp(1671546600);

    /// Bootstrap timestamp for the testnet chain
    pub static ref TESTNET_BOOTSTRAP_TIMESTAMP: Timestamp = Timestamp(1671546600);

    // Commonly used Float10
    pub static ref  FLOAT10_ZERO: Float10 = Float10::from_str_native("0").unwrap().with_precision(RADIX_BITS).value();
    pub static ref  FLOAT10_ONE: Float10 = Float10::from_str_native("1").unwrap().with_precision(RADIX_BITS).value();
    pub static ref  FLOAT10_TWO: Float10 = Float10::from_str_native("2").unwrap().with_precision(RADIX_BITS).value();
    pub static ref  FLOAT10_THREE: Float10 = Float10::from_str_native("3").unwrap().with_precision(RADIX_BITS).value();
    pub static ref  FLOAT10_FIVE: Float10 = Float10::from_str_native("5").unwrap().with_precision(RADIX_BITS).value();
    pub static ref  FLOAT10_NINE: Float10 = Float10::from_str_native("9").unwrap().with_precision(RADIX_BITS).value();
    pub static ref  FLOAT10_TEN: Float10 = Float10::from_str_native("10").unwrap().with_precision(RADIX_BITS).value();

    // Consensus parameters
    pub static ref  DT: Float10 =  Float10::from_str_native("0.1").unwrap().with_precision(RADIX_BITS).value();
    pub static ref  TI: Float10 = FLOAT10_ONE.clone();
    pub static ref  TD: Float10 = FLOAT10_ONE.clone();
    pub static ref  KP: Float10 = Float10::from_str_native("0.1").unwrap().with_precision(RADIX_BITS).value();
    pub static ref  KI: Float10 = Float10::from_str_native("0.03").unwrap().with_precision(RADIX_BITS).value();
    pub static ref  KD: Float10 = FLOAT10_ONE.clone();
    pub static ref PID_OUT_STEP: Float10  = Float10::from_str_native("0.1").unwrap().with_precision(RADIX_BITS).value();
    pub static ref MAX_DER: Float10 = Float10::from_str_native("0.1").unwrap().with_precision(RADIX_BITS).value();
    pub static ref MIN_DER: Float10 = Float10::from_str_native("-0.1").unwrap().with_precision(RADIX_BITS).value();
    pub static ref MAX_F: Float10 = Float10::from_str_native("0.99").unwrap().with_precision(RADIX_BITS).value();
    pub static ref MIN_F: Float10 = Float10::from_str_native("0.05").unwrap().with_precision(RADIX_BITS).value();
    pub static ref DEG_RATE: Float10 = Float10::from_str_native("0.9").unwrap().with_precision(RADIX_BITS).value();

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
pub const SLOT_TIME: u64 = 90;

/// Finalization sync period duration (should be >=2/3 of slot time)
pub const FINAL_SYNC_DUR: u64 = 60;

/// Max resync retries duration in epochs
pub const SYNC_RETRIES_DURATION: u64 = 2;

/// Max resync retries
pub const SYNC_MAX_RETRIES: u64 = 10;

/// Transactions included in a block cap
pub const TXS_CAP: usize = 50;

/// Block leader reward
pub const REWARD: u64 = 1;

/// Leader proofs k for zk proof rows (rows=2^k)
pub const LEADER_PROOF_K: u32 = 13;

// TODO: Describe these constants
pub const RADIX_BITS: usize = 76;

pub const P: &str = "28948022309329048855892746252171976963363056481941560715954676764349967630337";
pub const LOTTERY_HEAD_START: u64 = 1;
pub const PRF_NULLIFIER_PREFIX: u64 = 0;
pub const PI_COMMITMENT_X_INDEX: usize = 1;
pub const PI_COMMITMENT_Y_INDEX: usize = 2;
pub const PI_COMMITMENT_ROOT: usize = 5;
pub const PI_NULLIFIER_INDEX: usize = 7;
pub const PI_MU_Y_INDEX: usize = 8;
pub const PI_MU_RHO_INDEX: usize = 10;
pub const PI_SIGMA1_INDEX: usize = 12;
pub const PI_SIGMA2_INDEX: usize = 13;
pub const GENESIS_TOTAL_STAKE: i64 = 1;
