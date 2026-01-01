/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! DarkFi Money Contract
//!
//! Smart contract implementing money transfers, atomic swaps, token
//! minting and freezing, and staking/unstaking of consensus tokens.

use darkfi_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
#[derive(Debug)]
// ANCHOR: money-function
pub enum MoneyFunction {
    FeeV1 = 0x00,
    GenesisMintV1 = 0x01,
    PoWRewardV1 = 0x02,
    TransferV1 = 0x03,
    OtcSwapV1 = 0x04,
    AuthTokenMintV1 = 0x05,
    AuthTokenFreezeV1 = 0x06,
    TokenMintV1 = 0x07,
}
// ANCHOR_END: money-function

impl TryFrom<u8> for MoneyFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::FeeV1),
            0x01 => Ok(Self::GenesisMintV1),
            0x02 => Ok(Self::PoWRewardV1),
            0x03 => Ok(Self::TransferV1),
            0x04 => Ok(Self::OtcSwapV1),
            0x05 => Ok(Self::AuthTokenMintV1),
            0x06 => Ok(Self::AuthTokenFreezeV1),
            0x07 => Ok(Self::TokenMintV1),
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
pub const MONEY_CONTRACT_NULLIFIER_ROOTS_TREE: &str = "nullifier_roots";
pub const MONEY_CONTRACT_TOKEN_FREEZE_TREE: &str = "token_freezes";
pub const MONEY_CONTRACT_FEES_TREE: &str = "fees";

// These are keys inside the info tree
pub const MONEY_CONTRACT_DB_VERSION: &[u8] = b"db_version";
pub const MONEY_CONTRACT_COIN_MERKLE_TREE: &[u8] = b"coins_tree";
pub const MONEY_CONTRACT_LATEST_COIN_ROOT: &[u8] = b"last_coins_root";
pub const MONEY_CONTRACT_LATEST_NULLIFIER_ROOT: &[u8] = b"last_nullifiers_root";

/// Precalculated root hash for a tree containing only a single Fp::ZERO coin.
/// Used to save gas.
pub const EMPTY_COINS_TREE_ROOT: [u8; 32] = [
    0xb8, 0xc1, 0x07, 0x5a, 0x80, 0xa8, 0x09, 0x65, 0xc2, 0x39, 0x8f, 0x71, 0x1f, 0xe7, 0x3e, 0x05,
    0xb4, 0xed, 0xae, 0xde, 0xf1, 0x62, 0xf2, 0x61, 0xd4, 0xee, 0xd7, 0xcd, 0x72, 0x74, 0x8d, 0x17,
];

/// zkas fee circuit namespace
pub const MONEY_CONTRACT_ZKAS_FEE_NS_V1: &str = "Fee_V1";
/// zkas mint circuit namespace
pub const MONEY_CONTRACT_ZKAS_MINT_NS_V1: &str = "Mint_V1";
/// zkas burn circuit namespace
pub const MONEY_CONTRACT_ZKAS_BURN_NS_V1: &str = "Burn_V1";
/// zkas token auth mint circuit namespace
pub const MONEY_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1: &str = "AuthTokenMint_V1";
/// zkas token mint circuit namespace
pub const MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1: &str = "TokenMint_V1";
