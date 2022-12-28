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

use lazy_static::lazy_static;
use pasta_curves::{group::ff::Field, pallas};
use rand::rngs::OsRng;

pub mod validate;
/// This is an anonymous contract function that mutates the internal DAO state.
///
/// Corresponds to `mint(proposer_limit, quorum, approval_ratio, dao_pubkey, dao_blind)`
///
/// The prover creates a `Builder`, which then constructs the `Tx` that the verifier can
/// check using `state_transition()`.
///
/// # Arguments
///
/// * `proposer_limit` - Number of governance tokens that holder must possess in order to
///   propose a new vote.
/// * `quorum` - Number of minimum votes that must be met for a proposal to pass.
/// * `approval_ratio` - Ratio of winning to total votes for a proposal to pass.
/// * `dao_pubkey` - Public key of the DAO for permissioned access. This can also be
///   shared publicly if you want a full decentralized DAO.
/// * `dao_blind` - Blinding factor for the DAO bulla.
///
/// # Example
///
/// ```rust
/// let dao_proposer_limit = 110;
/// let dao_quorum = 110;
/// let dao_approval_ratio = 2;
///
/// let builder = dao_contract::Mint::Builder(
///     dao_proposer_limit,
///     dao_quorum,
///     dao_approval_ratio,
///     gov_token_id,
///     dao_pubkey,
///     dao_blind
/// );
/// let tx = builder.build();
/// ```
pub mod wallet;

lazy_static! {
    pub static ref FUNC_ID: pallas::Base = pallas::Base::random(&mut OsRng);
}
