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

//! Smart contract implementing Anonymous DAOs on DarkFi.

use darkfi_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
#[derive(PartialEq, Debug)]
pub enum DaoFunction {
    Mint = 0x00,
    Propose = 0x01,
    Vote = 0x02,
    Exec = 0x03,
    AuthMoneyTransfer = 0x04,
}

impl TryFrom<u8> for DaoFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(DaoFunction::Mint),
            0x01 => Ok(DaoFunction::Propose),
            0x02 => Ok(DaoFunction::Vote),
            0x03 => Ok(DaoFunction::Exec),
            0x04 => Ok(DaoFunction::AuthMoneyTransfer),
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
pub const DAO_CONTRACT_INFO_TREE: &str = "info";
pub const DAO_CONTRACT_BULLAS_TREE: &str = "bullas";
pub const DAO_CONTRACT_MERKLE_ROOTS_TREE: &str = "roots";
pub const DAO_CONTRACT_PROPOSAL_BULLAS_TREE: &str = "proposals";
pub const DAO_CONTRACT_VOTE_NULLIFIERS_TREE: &str = "vote_nullifiers";

// These are keys inside the info tree
pub const DAO_CONTRACT_DB_VERSION: &[u8] = b"db_version";
pub const DAO_CONTRACT_MERKLE_TREE: &[u8] = b"merkle_tree";
pub const DAO_CONTRACT_LATEST_ROOT: &[u8] = b"last_root";

/// zkas dao mint circuit namespace
pub const DAO_CONTRACT_ZKAS_MINT_NS: &str = "Mint";
/// zkas dao vote input circuit namespace
pub const DAO_CONTRACT_ZKAS_VOTE_INPUT_NS: &str = "VoteInput";
/// zkas dao vote main circuit namespace
pub const DAO_CONTRACT_ZKAS_VOTE_MAIN_NS: &str = "VoteMain";
/// zkas dao propose input circuit namespace
pub const DAO_CONTRACT_ZKAS_PROPOSE_INPUT_NS: &str = "ProposeInput";
/// zkas dao propose main circuit namespace
pub const DAO_CONTRACT_ZKAS_PROPOSE_MAIN_NS: &str = "ProposeMain";
/// zkas dao exec circuit namespace
pub const DAO_CONTRACT_ZKAS_EXEC_NS: &str = "Exec";
/// zkas dao early exec circuit namespace
pub const DAO_CONTRACT_ZKAS_EARLY_EXEC_NS: &str = "EarlyExec";
/// zkas dao auth money_transfer circuit namespace
pub const DAO_CONTRACT_ZKAS_AUTH_MONEY_TRANSFER_NS: &str = "AuthMoneyTransfer";
/// zkas dao auth money_transfer encrypted coin circuit namespace
pub const DAO_CONTRACT_ZKAS_AUTH_MONEY_TRANSFER_ENC_COIN_NS: &str = "AuthMoneyTransferEncCoin";

/// Not allowed to make proposals using snapshots with block heights older than this depth
pub const PROPOSAL_SNAPSHOT_CUTOFF_LIMIT: u32 = 100;

// ANCHOR: dao-blockwindow
const _SECS_IN_HOUR: u64 = 60 * 60;
const _WINDOW_TIME_HR: u64 = 4;
// Precalculating the const for better performance
// WINDOW_TIME = WINDOW_TIME_HR * SECS_IN_HOUR
const WINDOW_TIME: u64 = 14400;

/// Blockwindow from block height and target time. Used for time limit on DAO proposals.
pub fn blockwindow(height: u32, target: u32) -> u64 {
    let timestamp_secs = (height as u64) * (target as u64);
    timestamp_secs / WINDOW_TIME
}
// ANCHOR_END: dao-blockwindow
