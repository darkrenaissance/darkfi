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

use darkfi_sdk::error::ContractError;

#[derive(Debug, Clone, thiserror::Error)]
pub enum DeployError {
    #[error("Contract deployment is locked.")]
    ContractLocked,

    #[error("Contract does not exist.")]
    ContractNonExistent,

    #[error("WASM bincode invalid.")]
    WasmBincodeInvalid,
}

impl From<DeployError> for ContractError {
    fn from(e: DeployError) -> Self {
        match e {
            DeployError::ContractLocked => Self::Custom(1),
            DeployError::ContractNonExistent => Self::Custom(2),
            DeployError::WasmBincodeInvalid => Self::Custom(3),
        }
    }
}
