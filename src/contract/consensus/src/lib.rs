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

//! Smart contract implementing staking, unstaking and evolving
//! of consensus tokens.

//! Smart contract implementing money transfers, atomic swaps, token
//! minting and freezing, and staking/unstaking of consensus tokens.

use darkfi_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
pub enum ConsensusFunction {
    StakeV1 = 0x00,
    //EvolveV1 = 0x01,
    //UnstakeV1 = 0x02,
}

impl TryFrom<u8> for ConsensusFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::StakeV1),
            //0x01 => Ok(Self::EvolveV1),
            //0x02 => Ok(Self::UnstakeV1),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Call parameters definitions
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for interaction with this smart contract
pub mod client;

// These are the different sled trees that will be created
pub const CONSENSUS_CONTRACT_INFO_TREE: &str = "info";
pub const CONSENSUS_CONTRACT_COINS_TREE: &str = "coins";
pub const CONSENSUS_CONTRACT_COIN_ROOTS_TREE: &str = "coin_roots";
pub const CONSENSUS_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";

// These are keys inside the info tree
pub const CONSENSUS_CONTRACT_DB_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const CONSENSUS_CONTRACT_COIN_MERKLE_TREE: &str = "coin_tree";

/// zkas mint circuit namespace
pub const CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1: &str = "Consensus_Mint_V1";
pub const CONSENSUS_CONTRACT_ZKAS_BURN_NS_V1: &str = "Consensus_Burn_V1";
