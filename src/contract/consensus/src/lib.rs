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

use darkfi_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
pub enum ConsensusFunction {
    GenesisStakeV1 = 0x00,
    StakeV1 = 0x01,
    ProposalBurnV1 = 0x02,
    ProposalRewardV1 = 0x03,
    ProposalMintV1 = 0x04,
    UnstakeV1 = 0x05,
}

impl TryFrom<u8> for ConsensusFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::GenesisStakeV1),
            0x01 => Ok(Self::StakeV1),
            0x02 => Ok(Self::ProposalBurnV1),
            0x03 => Ok(Self::ProposalRewardV1),
            0x04 => Ok(Self::ProposalMintV1),
            0x05 => Ok(Self::UnstakeV1),
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
