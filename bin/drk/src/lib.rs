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

/// Main CLI-util structure
mod drk;
pub use drk::Drk;

/// Error codes
pub mod error;

/// darkfid JSON-RPC related methods
pub mod rpc;

/// Payment methods
pub mod transfer;

/// Swap methods
pub mod swap;

/// Token methods
pub mod token;

/// CLI utility functions
pub mod cli_util;

/// Wallet functionality related to Money
pub mod money;

/// Wallet functionality related to Dao
pub mod dao;

/// Wallet functionality related to Deployooor
pub mod deploy;

/// Wallet functionality related to transactions history
pub mod txs_history;

/// Wallet database operations handler
pub mod walletdb;
