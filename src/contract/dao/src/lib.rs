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

use darkfi_sdk::error::ContractError;

#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;

pub mod state;

#[cfg(feature = "client")]
pub mod note;

#[cfg(feature = "client")]
/// Transaction building API for clients interacting with DAO contract
pub mod dao_client;

#[cfg(feature = "client")]
/// Transaction building API for clients interacting with DAO contract
pub mod dao_propose_client;

#[cfg(feature = "client")]
/// Transaction building API for clients interacting with DAO contract
pub mod dao_vote_client;

#[cfg(feature = "client")]
/// Transaction building API for clients interacting with money contract
pub mod money_client;

// These are the zkas circuit namespaces
pub const DAO_CONTRACT_ZKAS_DAO_MINT_NS: &str = "DaoMint";
pub const DAO_CONTRACT_ZKAS_DAO_EXEC_NS: &str = "DaoExec";
pub const DAO_CONTRACT_ZKAS_DAO_VOTE_BURN_NS: &str = "DaoVoteInput";
pub const DAO_CONTRACT_ZKAS_DAO_VOTE_MAIN_NS: &str = "DaoVoteMain";
pub const DAO_CONTRACT_ZKAS_DAO_PROPOSE_BURN_NS: &str = "DaoProposeInput";
pub const DAO_CONTRACT_ZKAS_DAO_PROPOSE_MAIN_NS: &str = "DaoProposeMain";

#[repr(u8)]
pub enum DaoFunction {
    Mint = 0x00,
    Propose = 0x01,
    Vote = 0x02,
    Exec = 0x03,
}

impl TryFrom<u8> for DaoFunction {
    type Error = ContractError;

    fn try_from(x: u8) -> core::result::Result<DaoFunction, Self::Error> {
        match x {
            0x00 => Ok(DaoFunction::Mint),
            0x01 => Ok(DaoFunction::Propose),
            0x02 => Ok(DaoFunction::Vote),
            0x03 => Ok(DaoFunction::Exec),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}
