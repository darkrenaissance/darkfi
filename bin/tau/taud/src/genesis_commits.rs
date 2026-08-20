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

use darkfi_sdk::{crypto::pasta_prelude::PrimeField, pasta::pallas};

/// Return Taud's configured pregenerated RLN commitment set.
pub fn pregenerated_identity_commitments() -> Vec<[u8; 32]> {
    TAUD_GENESIS_COMMITMENTS_REPR.to_vec()
}

/// Check whether an RLN commitment belongs to Taud's pregenerated set.
pub fn is_pregenerated_commitment(commitment: &pallas::Base) -> bool {
    TAUD_GENESIS_COMMITMENTS_REPR.contains(&commitment.to_repr())
}

/// Taud's pregenerated RLN commitment set, represented as an array of 32-byte arrays.
/// TODO: Populate this with the actual pregenerated commitments once they are available.
pub const TAUD_GENESIS_COMMITMENTS_REPR: &[[u8; 32]] = &[];
