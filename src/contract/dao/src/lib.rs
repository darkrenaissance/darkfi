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

//! Smart contract implementing Anonymous DAOs on DarkFi

use darkfi_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
pub enum DaoFunction {
    Mint = 0x00,
    Propose = 0x01,
    Vote = 0x02,
    Exec = 0x03,
}

impl TryFrom<u8> for DaoFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(DaoFunction::Mint),
            0x01 => Ok(DaoFunction::Propose),
            0x02 => Ok(DaoFunction::Vote),
            0x03 => Ok(DaoFunction::Exec),
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

// TODO: Delete these and use the proper API
#[cfg(feature = "client")]
pub mod money_client;
#[cfg(feature = "client")]
pub mod wallet_cache;

// These are the different sled trees that will be created
pub const DAO_CONTRACT_DB_INFO_TREE: &str = "dao_info";
pub const DAO_CONTRACT_DB_DAO_BULLAS: &str = "dao_bullas";
pub const DAO_CONTRACT_DB_DAO_MERKLE_ROOTS: &str = "dao_roots";
pub const DAO_CONTRACT_DB_PROPOSAL_BULLAS: &str = "dao_proposals";
pub const DAO_CONTRACT_DB_VOTE_NULLIFIERS: &str = "dao_vote_nullifiers";

// These are keys inside the info tree
pub const DAO_CONTRACT_KEY_DB_VERSION: &str = "db_version";
pub const DAO_CONTRACT_KEY_DAO_MERKLE_TREE: &str = "dao_merkle_tree";

/// zkas dao mint circuit namespace
pub const DAO_CONTRACT_ZKAS_DAO_MINT_NS: &str = "DaoMint";
/// zkas dao exec circuit namespace
pub const DAO_CONTRACT_ZKAS_DAO_EXEC_NS: &str = "DaoExec";
/// zkas dao vote input circuit namespace
pub const DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS: &str = "DaoVoteInput";
/// zkas dao vote main circuit namespace
pub const DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS: &str = "DaoVoteMain";
/// zkas dao propose input circuit namespace
pub const DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS: &str = "DaoProposeInput";
/// zkas dao propose main circuit namespace
pub const DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS: &str = "DaoProposeMain";
