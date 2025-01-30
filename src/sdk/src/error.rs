/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
#[derive(Debug, Clone, thiserror::Error)]
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

    #[error("Error setting return value")]
    SetRetvalError,

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

    #[error("Db not found")]
    DbNotFound,

    #[error("Db set failed")]
    DbSetFailed,

    #[error("Db del failed")]
    DbDelFailed,

    #[error("Db lookup failed")]
    DbLookupFailed,

    #[error("Db get failed")]
    DbGetFailed,

    #[error("Db get empty")]
    DbGetEmpty,

    #[error("Db contains_key failed")]
    DbContainsKeyFailed,

    #[error("Invalid function call")]
    InvalidFunction,

    #[error("SMT: put failed with storage backend")]
    SmtPutFailed,

    #[error("SMT: rm failed with storage backend")]
    SmtDelFailed,

    #[error("Error retrieving system time")]
    GetSystemTimeFailed,

    // Provide feedback when the data sent is too large. For example,
    // if a user tries to send > u32::MAX bytes, we can limit the
    // size and present this error.
    #[error("Data too large")]
    DataTooLarge,

    #[error("Hex string is not properly formatted")]
    HexFmtErr,
}

/// Builtin return values occupy the upper 32 bits
macro_rules! to_builtin {
    ($error:expr) => {
        i64::MIN + $error
    };
}

pub const CUSTOM_ZERO: i64 = to_builtin!(1);
pub const INTERNAL_ERROR: i64 = to_builtin!(2);
pub const SET_RETVAL_ERROR: i64 = to_builtin!(3);
pub const IO_ERROR: i64 = to_builtin!(4);
pub const NULLIFIER_EXIST_CHECK: i64 = to_builtin!(5);
pub const VALID_MERKLE_CHECK: i64 = to_builtin!(6);
pub const UPDATE_ALREADY_SET: i64 = to_builtin!(7);
pub const DB_INIT_FAILED: i64 = to_builtin!(8);
pub const CALLER_ACCESS_DENIED: i64 = to_builtin!(9);
pub const DB_NOT_FOUND: i64 = to_builtin!(10);
pub const DB_SET_FAILED: i64 = to_builtin!(11);
pub const DB_LOOKUP_FAILED: i64 = to_builtin!(12);
pub const DB_GET_FAILED: i64 = to_builtin!(13);
pub const DB_GET_EMPTY: i64 = to_builtin!(14);
pub const DB_CONTAINS_KEY_FAILED: i64 = to_builtin!(15);
pub const INVALID_FUNCTION: i64 = to_builtin!(16);
pub const DB_DEL_FAILED: i64 = to_builtin!(17);
pub const SMT_PUT_FAILED: i64 = to_builtin!(18);
pub const SMT_DEL_FAILED: i64 = to_builtin!(19);
pub const GET_SYSTEM_TIME_FAILED: i64 = to_builtin!(20);
pub const DATA_TOO_LARGE: i64 = to_builtin!(21);
pub const HEX_FMT_ERR: i64 = to_builtin!(22);

impl From<ContractError> for i64 {
    fn from(err: ContractError) -> Self {
        match err {
            ContractError::Internal => INTERNAL_ERROR,
            ContractError::IoError(_) => IO_ERROR,
            ContractError::SetRetvalError => SET_RETVAL_ERROR,
            ContractError::NullifierExistCheck => NULLIFIER_EXIST_CHECK,
            ContractError::ValidMerkleCheck => VALID_MERKLE_CHECK,
            ContractError::UpdateAlreadySet => UPDATE_ALREADY_SET,
            ContractError::DbInitFailed => DB_INIT_FAILED,
            ContractError::CallerAccessDenied => CALLER_ACCESS_DENIED,
            ContractError::DbNotFound => DB_NOT_FOUND,
            ContractError::DbSetFailed => DB_SET_FAILED,
            ContractError::DbLookupFailed => DB_LOOKUP_FAILED,
            ContractError::DbGetFailed => DB_GET_FAILED,
            ContractError::DbGetEmpty => DB_GET_EMPTY,
            ContractError::DbContainsKeyFailed => DB_CONTAINS_KEY_FAILED,
            ContractError::InvalidFunction => INVALID_FUNCTION,
            ContractError::DbDelFailed => DB_DEL_FAILED,
            ContractError::SmtPutFailed => SMT_PUT_FAILED,
            ContractError::SmtDelFailed => SMT_DEL_FAILED,
            ContractError::GetSystemTimeFailed => GET_SYSTEM_TIME_FAILED,
            ContractError::DataTooLarge => DATA_TOO_LARGE,
            ContractError::HexFmtErr => HEX_FMT_ERR,
            ContractError::Custom(error) => {
                if error == 0 {
                    CUSTOM_ZERO
                } else {
                    error as i64
                }
            }
        }
    }
}

impl From<i64> for ContractError {
    fn from(error: i64) -> Self {
        match error {
            CUSTOM_ZERO => Self::Custom(0),
            INTERNAL_ERROR => Self::Internal,
            SET_RETVAL_ERROR => Self::SetRetvalError,
            IO_ERROR => Self::IoError("Unknown".to_string()),
            NULLIFIER_EXIST_CHECK => Self::NullifierExistCheck,
            VALID_MERKLE_CHECK => Self::ValidMerkleCheck,
            UPDATE_ALREADY_SET => Self::UpdateAlreadySet,
            DB_INIT_FAILED => Self::DbInitFailed,
            CALLER_ACCESS_DENIED => Self::CallerAccessDenied,
            DB_NOT_FOUND => Self::DbNotFound,
            DB_SET_FAILED => Self::DbSetFailed,
            DB_LOOKUP_FAILED => Self::DbLookupFailed,
            DB_GET_FAILED => Self::DbGetFailed,
            DB_GET_EMPTY => Self::DbGetEmpty,
            DB_CONTAINS_KEY_FAILED => Self::DbContainsKeyFailed,
            INVALID_FUNCTION => Self::InvalidFunction,
            DB_DEL_FAILED => Self::DbDelFailed,
            SMT_PUT_FAILED => Self::SmtPutFailed,
            SMT_DEL_FAILED => Self::SmtDelFailed,
            GET_SYSTEM_TIME_FAILED => Self::GetSystemTimeFailed,
            DATA_TOO_LARGE => Self::DataTooLarge,
            HEX_FMT_ERR => Self::HexFmtErr,
            _ => Self::Custom(error as u32),
        }
    }
}

impl From<std::io::Error> for ContractError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(format!("{}", err))
    }
}

impl From<bs58::decode::Error> for ContractError {
    fn from(err: bs58::decode::Error) -> Self {
        Self::IoError(format!("{}", err))
    }
}

/// Main result type used by DarkTree.
pub type DarkTreeResult<T> = ResultGeneric<T, DarkTreeError>;

/// General DarkTree related errors.
#[derive(Debug, Clone, thiserror::Error)]
pub enum DarkTreeError {
    #[error("Invalid DarkLeaf index found: {0} (Expected: {1}")]
    InvalidLeafIndex(usize, usize),

    #[error("Invalid DarkLeaf parent index found for leaf: {0}")]
    InvalidLeafParentIndex(usize),

    #[error("Invalid DarkLeaf children index found for leaf: {0}")]
    InvalidLeafChildrenIndexes(usize),

    #[error("Invalid DarkTree min capacity found: {0} (Expected: >= 1)")]
    InvalidMinCapacity(usize),

    #[error("DarkTree min capacity has not been exceeded")]
    MinCapacityNotExceeded,

    #[error("Invalid DarkTree max capacity found: {0} (Expected: >= {1})")]
    InvalidMaxCapacity(usize, usize),

    #[error("DarkTree max capacity has been exceeded")]
    MaxCapacityExceeded,
}
