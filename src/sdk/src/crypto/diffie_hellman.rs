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

use super::{util::mod_r_p, PublicKey, SecretKey};

pub const KDF_SAPLING_PERSONALIZATION: &[u8; 16] = b"DarkFiSaplingKDF";

/// Sapling key agreement for note encryption.
/// Implements section 5.4.4.3 of the Zcash Protocol Specification
pub fn sapling_ka_agree(esk: &SecretKey, pk_d: &PublicKey) -> PublicKey {
    let esk_s = mod_r_p(esk.inner());
    let mut wnaf = Wnaf::new();
    PublicKey::from(wnaf.scalar(&esk_s).base(pk_d.inner()).clear_cofactor())
}

/// Sapling KDF for note encryption.
pub fn kdf_sapling(dhsecret: &PublicKey, epk: &PublicKey) -> Blake2bHash {
    Blake2bParams::new()
        .hash_length(32)
        .personal(KDF_SAPLING_PERSONALIZATION)
        .to_state()
        .update(&dhsecret.inner().to_bytes())
        .update(&epk.inner().to_bytes())
        .finalize()
}
