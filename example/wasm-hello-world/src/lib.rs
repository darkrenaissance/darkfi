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

use darkfi_sdk::{error::ContractError, pasta::pallas};
use darkfi_serial::{SerialDecodable, SerialEncodable};

#[cfg(feature = "client")]
use darkfi_serial::async_trait;

/// Functions available in the contract
#[repr(u8)]
pub enum ContractFunction {
    Register = 0x00,
    Deregister = 0x01,
}

impl TryFrom<u8> for ContractFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::Register),
            0x01 => Ok(Self::Deregister),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Function parameters
#[derive(Debug, Clone, Copy, SerialEncodable, SerialDecodable)]
pub struct HelloParams {
    /// X coordinate of the public key
    pub x: pallas::Base,
    /// Y coordinate of the public key
    pub y: pallas::Base,
}

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

/// This is a sled tree that will be created
pub const HELLO_CONTRACT_MEMBER_TREE: &str = "members";

/// zkas circuit namespace
pub const HELLO_CONTRACT_ZKAS_SECRETCOMMIT_NS: &str = "SecretCommitment";
