/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

//! Smart contract implementing non-native smart contract deployment.

use darkfi_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
pub enum DeployFunction {
    DeployV1 = 0x00,
    LockV1 = 0x01,
}

impl TryFrom<u8> for DeployFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::DeployV1),
            0x01 => Ok(Self::LockV1),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

/// Call parameters definitions
pub mod model;

/// Contract errors
pub mod error;

#[cfg(feature = "client")]
/// Client API for interaction with this smart contract
pub mod client;

// These are the different sled trees that will be created
pub const DEPLOY_CONTRACT_INFO_TREE: &str = "info";
pub const DEPLOY_CONTRACT_LOCK_TREE: &str = "lock";

// These are keys inside the info tree
pub const DEPLOY_CONTRACT_DB_VERSION: &[u8] = b"db_version";
