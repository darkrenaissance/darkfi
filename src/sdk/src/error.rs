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

use std::result::Result as ResultGeneric;

pub type GenericResult<T> = ResultGeneric<T, ContractError>;
pub type ContractResult = ResultGeneric<(), ContractError>;

/// Error codes available in the contract.
#[derive(Debug, thiserror::Error)]
pub enum ContractError {
    /// Allows on-chain programs to implement contract-specific error types and
    /// see them returned by the runtime. A contract-specific error may be any
    /// type that is represented as or serialized to an u32 integer.
    #[error("Custom contract error: {0:#x}")]
    Custom(u32),

    #[error("Internal error")]
    Internal,

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Error setting update")]
    SetUpdateError,

    #[error("Error checking if nullifier exists")]
    NullifierExistCheck,

    #[error("Error checking merkle root validity")]
    ValidMerkleCheck,

    #[error("Update already set")]
    UpdateAlreadySet,

    #[error("Db init failed")]
    DbInitFailed,

    #[error("Caller access was denied")]
    CallerAccessDenied,
}

/// Builtin return values occupy the upper 32 bits
const BUILTIN_BIT_SHIFT: usize = 32;
macro_rules! to_builtin {
    ($error:expr) => {
        ($error as u64) << BUILTIN_BIT_SHIFT
    };
}

pub const CUSTOM_ZERO: u64 = to_builtin!(1);
pub const INTERNAL_ERROR: u64 = to_builtin!(2);
pub const SET_UPDATE_ERROR: u64 = to_builtin!(3);
pub const IO_ERROR: u64 = to_builtin!(4);
pub const NULLIFIER_EXIST_CHECK: u64 = to_builtin!(5);
pub const VALID_MERKLE_CHECK: u64 = to_builtin!(6);
pub const UPDATE_ALREADY_SET: u64 = to_builtin!(7);
pub const DB_INIT_FAILED: u64 = to_builtin!(8);
pub const CALLER_ACCESS_DENIED: u64 = to_builtin!(9);

impl From<ContractError> for u64 {
    fn from(err: ContractError) -> Self {
        match err {
            ContractError::Internal => INTERNAL_ERROR,
            ContractError::IoError(_) => IO_ERROR,
            ContractError::SetUpdateError => SET_UPDATE_ERROR,
            ContractError::NullifierExistCheck => NULLIFIER_EXIST_CHECK,
            ContractError::ValidMerkleCheck => VALID_MERKLE_CHECK,
            ContractError::UpdateAlreadySet => UPDATE_ALREADY_SET,
            ContractError::DbInitFailed => DB_INIT_FAILED,
            ContractError::CallerAccessDenied => CALLER_ACCESS_DENIED,
            ContractError::Custom(error) => {
                if error == 0 {
                    CUSTOM_ZERO
                } else {
                    error as u64
                }
            }
        }
    }
}

impl From<u64> for ContractError {
    fn from(error: u64) -> Self {
        match error {
            CUSTOM_ZERO => Self::Custom(0),
            INTERNAL_ERROR => Self::Internal,
            SET_UPDATE_ERROR => Self::SetUpdateError,
            IO_ERROR => Self::IoError("Unknown".to_string()),
            NULLIFIER_EXIST_CHECK => Self::NullifierExistCheck,
            VALID_MERKLE_CHECK => Self::ValidMerkleCheck,
            UPDATE_ALREADY_SET => Self::UpdateAlreadySet,
            DB_INIT_FAILED => Self::DbInitFailed,
            CALLER_ACCESS_DENIED => Self::CallerAccessDenied,
            _ => Self::Custom(error as u32),
        }
    }
}

impl From<std::io::Error> for ContractError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(format!("{}", err))
    }
}
