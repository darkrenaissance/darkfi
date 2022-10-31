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

use blake2b_simd::{Hash as Blake2bHash, Params as Blake2bParams};
use pasta_curves::group::{cofactor::CofactorGroup, GroupEncoding, Wnaf};

use crate::crypto::{
    keypair::{PublicKey, SecretKey},
    util::mod_r_p,
};

pub const KDF_SAPLING_PERSONALIZATION: &[u8; 16] = b"DarkFiSaplingKDF";

/// Functions used for encrypting the note in transaction outputs.

/// Sapling key agreement for note encryption.
///
/// Implements section 5.4.4.3 of the Zcash Protocol Specification.
pub fn sapling_ka_agree(esk: &SecretKey, pk_d: &PublicKey) -> PublicKey {
    // [8 esk] pk_d
    // <ExtendedPoint as CofactorGroup>::clear_cofactor is implemented using
    // ExtendedPoint::mul_by_cofactor in the jubjub crate.

    // ExtendedPoint::multiply currently just implements double-and-add,
    // so using wNAF is a concrete speed improvement (as it operates over a window
    // of bits instead of individual bits).
    // We want that to be fast because it's in the hot path for trial decryption of
    // notes on chain.
    let esk_s = mod_r_p(esk.inner());
    let mut wnaf = Wnaf::new();
    PublicKey(wnaf.scalar(&esk_s).base(pk_d.0).clear_cofactor())
}

/// Sapling KDF for note encryption.
///
/// Implements section 5.4.4.4 of the Zcash Protocol Specification.
pub fn kdf_sapling(dhsecret: &PublicKey, epk: &PublicKey) -> Blake2bHash {
    Blake2bParams::new()
        .hash_length(32)
        .personal(KDF_SAPLING_PERSONALIZATION)
        .to_state()
        .update(&dhsecret.0.to_bytes())
        .update(&epk.0.to_bytes())
        .finalize()
}
