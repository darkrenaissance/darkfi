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

pub mod mint;
pub use mint::make_mint_call;

/// Provides core structs for DAO::propose()
///
/// * `DaoProposeStakeInput` are the staking inputs used to meet the `proposer_limit` threshold.
/// * `DaoProposeCall` is what creates the call data used on chain.
pub mod propose;
pub use propose::{DaoProposeCall, DaoProposeStakeInput};

/// Provides core structs for DAO::vote()
///
/// * `DaoVoteInput` are the inputs used in actual voting.
/// * `DaoVoteCall` is what creates the call data used on chain.
/// * `DaoVoteNote` is the secret shared info transmitted between DAO members.
pub mod vote;
pub use vote::{DaoVoteCall, DaoVoteInput};

pub mod exec;
pub use exec::DaoExecCall;

pub mod auth_xfer;
pub use auth_xfer::DaoAuthMoneyTransferCall;
