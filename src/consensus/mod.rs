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

/// Block definition
pub mod block;
pub use block::{Block, BlockInfo, BlockProposal, Header, ProposalChain};

/// Consensus metadata
pub mod metadata;
pub use metadata::{LeadProof, Metadata};

/// Consensus participant
pub mod participant;
pub use participant::Participant;

/// Consensus state
pub mod state;
pub use state::{ValidatorState, ValidatorStatePtr};

/// Utility functions and types
use crate::util::time::Timestamp;

/// P2P net protocols
pub mod proto;

/// async tasks to utilize the protocols
pub mod task;

/// Lamport clock
pub mod clock;
pub use clock::{Clock, Ticks};

/// Ouroboros simulation
pub mod ouroboros;

/// Ouroboros consensus coins functions
pub mod coins;

/// Utility types
pub mod types;
pub use types::Float10;

/// Utility functions
pub mod utils;

use lazy_static::lazy_static;
lazy_static! {
    /// Genesis hash for the mainnet chain
    pub static ref MAINNET_GENESIS_HASH_BYTES: blake3::Hash = blake3::hash(b"darkfi_mainnet");

    /// Genesis timestamp for the mainnet chain
    pub static ref MAINNET_GENESIS_TIMESTAMP: Timestamp = Timestamp(1650887115);

    /// Genesis hash for the testnet chain
    pub static ref TESTNET_GENESIS_HASH_BYTES: blake3::Hash = blake3::hash(b"darkfi_testnet");

    /// Genesis timestamp for the testnet chain
    pub static ref TESTNET_GENESIS_TIMESTAMP: Timestamp = Timestamp(1650887115);

    /// Block version number
    pub static ref BLOCK_VERSION: u8 = 1;

    /// Block magic bytes
    pub static ref BLOCK_MAGIC_BYTES: [u8; 4] = [0x11, 0x6d, 0x75, 0x1f];

    /// Block info magic bytes
    pub static ref BLOCK_INFO_MAGIC_BYTES: [u8; 4] = [0x90, 0x44, 0xf1, 0xf6];

    // Epoch configuration
    /// Slots in an epoch
    pub static ref EPOCH_LENGTH: u64 = 10;

    /// Block leader reward
    pub static ref REWARD: u64 = 420;

    /// `2 * DELTA` represents slot time
    pub static ref DELTA: u64 = 20;

    /// Leader proof rows number
    pub static ref LEADER_PROOF_K: u32 = 13;

    // TODO: Describe constants meaning in comment
    pub static ref RADIX_BITS: usize = 76;
    pub static ref P: &'static str = "28948022309329048855892746252171976963363056481941560715954676764349967630337";
    pub static ref LOTTERY_HEAD_START: u64 = 1;
    pub static ref PRF_NULLIFIER_PREFIX: u64 = 0;

}
