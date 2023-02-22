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
    TransferV1 = 0x00,
    OtcSwapV1 = 0x01,
    MintV1 = 0x02,
    FreezeV1 = 0x03,
    //Fee = 0x04,
    //Stake = 0x05,
    //Unstake = 0x06,
}

impl TryFrom<u8> for MoneyFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::TransferV1),
            0x01 => Ok(Self::OtcSwapV1),
            0x02 => Ok(Self::MintV1),
            0x03 => Ok(Self::FreezeV1),
            //0x04 => Ok(Self::Fee),
            //0x05 => Ok(Self::Stake),
            //0x06 => Ok(Self::Unstake),
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
pub const MONEY_CONTRACT_COIN_MERKLE_TREE: &str = "coin_tree";
pub const MONEY_CONTRACT_FAUCET_PUBKEYS: &str = "faucet_pubkeys";

/// zkas mint circuit namespace
pub const MONEY_CONTRACT_ZKAS_MINT_NS_V1: &str = "Mint_V1";
/// zkas burn circuit namespace
pub const MONEY_CONTRACT_ZKAS_BURN_NS_V1: &str = "Burn_V1";
/// zkas token mint circuit namespace
pub const MONEY_CONTRACT_ZKAS_TOKEN_MINT_NS_V1: &str = "TokenMint_V1";
/// zkas token freeze circuit namespace
pub const MONEY_CONTRACT_ZKAS_TOKEN_FRZ_NS_V1: &str = "TokenFreeze_V1";
