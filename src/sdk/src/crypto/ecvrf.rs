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

//! <https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-vrf-04#section-5>
#![allow(non_snake_case)]

#[cfg(feature = "async")]
use darkfi_serial::async_trait;
use darkfi_serial::{SerialDecodable, SerialEncodable};
use halo2_gadgets::ecc::chip::FixedPoint;
use pasta_curves::{
    arithmetic::CurveExt,
    group::{
        ff::{FromUniformBytes, PrimeField},
        Group, GroupEncoding,
    },
    pallas,
};

use super::{
    constants::NullifierK,
    util::{fp_mod_fv, hash_to_scalar},
    PublicKey, SecretKey,
};

/// Prefix domain used for `hash_to_curve` calls
const VRF_DOMAIN: &str = "DarkFi_ECVRF";

/// VRF Proof
#[derive(Copy, Clone, Debug, SerialEncodable, SerialDecodable)]
pub struct VrfProof {
    gamma: pallas::Point,
    c: blake3::Hash,
    s: pallas::Scalar,
}

impl VrfProof {
    /// Execute the VRF function and create a proof given a `SecretKey`
    /// and a seed input `alpha_string`.
    pub fn prove(x: SecretKey, alpha_string: &[u8]) -> Self {
        let Y = PublicKey::from_secret(x);

        let mut message = vec![];
        message.extend_from_slice(&Y.to_bytes());
        message.extend_from_slice(alpha_string);
        let H = pallas::Point::hash_to_curve(VRF_DOMAIN)(&message);

        let gamma = H * fp_mod_fv(x.inner());

        // Generate a determinnistic nonce
        let k = hash_to_scalar(VRF_DOMAIN.as_bytes(), &[&x.inner().to_repr(), &H.to_bytes()]);

        let mut hasher = blake3::Hasher::new();
        hasher.update(&H.to_bytes());
        hasher.update(&gamma.to_bytes());
        // The paper's B generator we use is NullifierK as that's used for
        // SecretKey -> PublicKey derivation.
        hasher.update(&(NullifierK.generator() * k).to_bytes());
        hasher.update(&(H * k).to_bytes());
        let c = hasher.finalize();

        let mut c_scalar = [0u8; 64];
        c_scalar[..blake3::OUT_LEN].copy_from_slice(c.as_bytes());
        let c_scalar = pallas::Scalar::from_uniform_bytes(&c_scalar);

        let s = k + c_scalar * fp_mod_fv(x.inner());

        Self { gamma, c, s }
    }

    /// Verify a `VrfProof` given a `Publickey` and a seed input `alpha_string`.
    pub fn verify(&self, Y: PublicKey, alpha_string: &[u8]) -> bool {
        let mut message = vec![];
        message.extend_from_slice(&Y.to_bytes());
        message.extend_from_slice(alpha_string);
        let H = pallas::Point::hash_to_curve(VRF_DOMAIN)(&message);

        let mut c = [0u8; 64];
        c[..blake3::OUT_LEN].copy_from_slice(self.c.as_bytes());
        let c_scalar = pallas::Scalar::from_uniform_bytes(&c);

        let U = NullifierK.generator() * self.s - Y.inner() * c_scalar;
        let V = H * self.s - self.gamma * c_scalar;

        let mut hasher = blake3::Hasher::new();
        hasher.update(&H.to_bytes());
        hasher.update(&self.gamma.to_bytes());
        hasher.update(&U.to_bytes());
        hasher.update(&V.to_bytes());

        hasher.finalize() == self.c
    }

    /// Returns the VRF output.
    /// **It is necessary** to do `VrfProof::verify` first in order to trust this function's output.
    /// TODO: FIXME: We should enforce verification before getting the output.
    pub fn hash_output(&self) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(VRF_DOMAIN.as_bytes());
        hasher.update(&[0x03]);
        hasher.update(&self.gamma.to_bytes());
        hasher.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    #[test]
    fn ecvrf() {
        // VRF secret key
        let secret_key = SecretKey::random(&mut OsRng);
        // VRF public key
        let public_key = PublicKey::from_secret(secret_key);
        // VRF input
        let input = [0xde, 0xad, 0xbe, 0xef];

        let proof = VrfProof::prove(secret_key, &input);
        assert!(proof.verify(public_key, &input));

        // Forged public key
        let forged_public_key = PublicKey::from_secret(SecretKey::random(&mut OsRng));
        assert!(!proof.verify(forged_public_key, &input));

        // Forged input
        let forged_input = [0xde, 0xad, 0xba, 0xbe];
        assert!(!proof.verify(public_key, &forged_input));
    }
}
