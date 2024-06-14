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

/// Result type used in the wallet database module
pub type WalletDbResult<T> = std::result::Result<T, WalletDbError>;

/// Custom wallet database errors available for drk.
/// Please sort them sensefully.
#[derive(Debug)]
pub enum WalletDbError {
    // Connection related errors
    ConnectionFailed = -32100,
    FailedToAquireLock = -32101,

    // Configuration related errors
    PragmaUpdateError = -32110,

    // Query execution related errors
    QueryPreparationFailed = -32120,
    QueryExecutionFailed = -32121,
    QueryFinalizationFailed = -32122,
    ParseColumnValueError = -32123,
    RowNotFound = -32124,

    // Generic error
    GenericError = -32130,
}

impl std::fmt::Display for WalletDbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WalletDbError::ConnectionFailed => write!(f, "WalletDbError::ConnectionFailed"),
            WalletDbError::FailedToAquireLock => write!(f, "WalletDbError::FailedToAquireLock"),
            WalletDbError::PragmaUpdateError => write!(f, "WalletDbError::PragmaUpdateError"),
            WalletDbError::QueryPreparationFailed => {
                write!(f, "WalletDbError::QueryPreparationFailed")
            }
            WalletDbError::QueryExecutionFailed => write!(f, "WalletDbError::QueryExecutionFailed"),
            WalletDbError::QueryFinalizationFailed => {
                write!(f, "WalletDbError::QueryFinalizationFailed")
            }
            WalletDbError::ParseColumnValueError => {
                write!(f, "WalletDbError::ParseColumnValueError")
            }
            WalletDbError::RowNotFound => write!(f, "WalletDbError::RowNotFound"),
            WalletDbError::GenericError => write!(f, "WalletDbError::GenericError"),
        }
    }
}

impl std::error::Error for WalletDbError {}
