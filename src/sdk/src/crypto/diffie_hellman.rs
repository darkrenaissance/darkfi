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

use blake2b_simd::{Hash as Blake2bHash, Params as Blake2bParams};
use pasta_curves::group::{GroupEncoding, Wnaf};

use super::{util::fp_mod_fv, PublicKey, SecretKey};
use crate::error::ContractError;

pub const KDF_SAPLING_PERSONALIZATION: &[u8; 16] = b"DarkFiSaplingKDF";

/// Sapling key agreement for note encryption.
/// Implements section 5.4.4.3 of the Zcash Protocol Specification
pub fn sapling_ka_agree(esk: &SecretKey, pk_d: &PublicKey) -> Result<PublicKey, ContractError> {
    let esk_s = fp_mod_fv(esk.inner());
    // Windowed multiplication is constant time. Hence that is used here vs naive EC mult.
    // Decrypting notes is a an amortized operation, so you want successful rare-case note
    // decryptions to be indistinguishable from the usual case.
    let mut wnaf = Wnaf::new();
    PublicKey::try_from(wnaf.scalar(&esk_s).base(pk_d.inner()))
}

/// Sapling KDF for note encryption.
pub fn kdf_sapling(dhsecret: &PublicKey, epk: &PublicKey) -> Blake2bHash {
    // The P.to_bytes() for P ∈ ℙₚ function used on affine curves it not perfectly constant time,
    // but it's close enough. The function returns 0 when P = ∞ is the identity which is the
    // edge case but almost never occurs.
    Blake2bParams::new()
        .hash_length(32)
        .personal(KDF_SAPLING_PERSONALIZATION)
        .to_state()
        .update(&dhsecret.inner().to_bytes())
        .update(&epk.inner().to_bytes())
        .finalize()
}
