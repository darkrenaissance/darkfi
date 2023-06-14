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

//! Smart contract implementing money transfers, atomic swaps, token
//! minting and freezing, and staking/unstaking of consensus tokens.

use darkfi_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
pub enum MoneyFunction {
    //Fee = 0x00,
    GenesisMintV1 = 0x01,
    TransferV1 = 0x02,
    OtcSwapV1 = 0x03,
    TokenMintV1 = 0x04,
    TokenFreezeV1 = 0x05,
    StakeV1 = 0x06,
    UnstakeV1 = 0x07,
}

impl TryFrom<u8> for MoneyFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            //0x00 => Ok(Self::Fee),
            0x01 => Ok(Self::GenesisMintV1),
            0x02 => Ok(Self::TransferV1),
            0x03 => Ok(Self::OtcSwapV1),
            0x04 => Ok(Self::TokenMintV1),
            0x05 => Ok(Self::TokenFreezeV1),
            0x06 => Ok(Self::StakeV1),
            0x07 => Ok(Self::UnstakeV1),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Internal contract errors
pub mod error;

/// Call parameters definitions
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for interaction with this smart contract
pub mod client;

// These are the different sled trees that will be created
pub const MONEY_CONTRACT_INFO_TREE: &str = "info";
pub const MONEY_CONTRACT_COINS_TREE: &str = "coins";
pub const MONEY_CONTRACT_COIN_ROOTS_TREE: &str = "coin_roots";
pub const MONEY_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const MONEY_CONTRACT_TOKEN_FREEZE_TREE: &str = "token_freezes";

// These are keys inside the info tree
pub const MONEY_CONTRACT_DB_VERSION: &str = "db_version";
pub const MONEY_CONTRACT_COIN_MERKLE_TREE: &str = "coin_tree";
pub const MONEY_CONTRACT_LATEST_COIN_ROOT: &str = "last_root";
pub const MONEY_CONTRACT_FAUCET_PUBKEYS: &str = "faucet_pubkeys";

/// zkas mint circuit namespace
pub const MONEY_CONTRACT_ZKAS_MINT_NS_V1: &str = "Mint_V1";
/// zkas burn circuit namespace
pub const MONEY_CONTRACT_ZKAS_BURN_NS_V1: &str = "Burn_V1";
/// zkas token mint circuit namespace
pub const MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1: &str = "TokenMint_V1";
/// zkas token freeze circuit namespace
pub const MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1: &str = "TokenFreeze_V1";

// These are the different sled trees that will be created
// for the consensus contract.
// We keep them here so we can reference them both in `Money`
// and `Consensus` contracts.
pub const CONSENSUS_CONTRACT_INFO_TREE: &str = "consensus_info";
pub const CONSENSUS_CONTRACT_NULLIFIERS_TREE: &str = "consensus_nullifiers";
pub const CONSENSUS_CONTRACT_STAKED_COINS_TREE: &str = "consensus_staked_coins";
pub const CONSENSUS_CONTRACT_UNSTAKED_COINS_TREE: &str = "consensus_unstaked_coins";
pub const CONSENSUS_CONTRACT_STAKED_COIN_ROOTS_TREE: &str = "consensus_staked_coin_roots";
pub const CONSENSUS_CONTRACT_UNSTAKED_COIN_ROOTS_TREE: &str = "consensus_unstaked_coin_roots";

// These are keys inside the consensus info tree
pub const CONSENSUS_CONTRACT_DB_VERSION: &str = "db_version";
pub const CONSENSUS_CONTRACT_STAKED_COIN_MERKLE_TREE: &str = "consensus_staked_coin_tree";
pub const CONSENSUS_CONTRACT_STAKED_COIN_LATEST_COIN_ROOT: &str = "consensus_staked_last_root";
pub const CONSENSUS_CONTRACT_UNSTAKED_COIN_MERKLE_TREE: &str = "consensus_unstaked_coin_tree";
pub const CONSENSUS_CONTRACT_UNSTAKED_COIN_LATEST_COIN_ROOT: &str = "consensus_unstaked_last_root";

/// zkas consensus mint circuit namespace
pub const CONSENSUS_CONTRACT_ZKAS_MINT_NS_V1: &str = "ConsensusMint_V1";
/// zkas consensus burn circuit namespace
pub const CONSENSUS_CONTRACT_ZKAS_BURN_NS_V1: &str = "ConsensusBurn_V1";
/// zkas proposal circuit namespace
pub const CONSENSUS_CONTRACT_ZKAS_PROPOSAL_NS_V1: &str = "ConsensusProposal_V1";
