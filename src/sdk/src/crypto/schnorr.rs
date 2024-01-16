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

#[cfg(feature = "async")]
use darkfi_serial::async_trait;
use darkfi_serial::{SerialDecodable, SerialEncodable};
use halo2_gadgets::ecc::chip::FixedPoint;
use pasta_curves::{
    group::{ff::Field, Group, GroupEncoding},
    pallas,
};
use rand_core::{CryptoRng, RngCore};

use super::{
    constants::{NullifierK, DRK_SCHNORR_DOMAIN},
    util::{hash_to_scalar, mod_r_p},
    PublicKey, SecretKey,
};

/// Schnorr signature with a commit and response
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Signature {
    commit: pallas::Point,
    response: pallas::Scalar,
}

impl Signature {
    /// Return a dummy identity `Signature`
    pub fn dummy() -> Self {
        Self { commit: pallas::Point::identity(), response: pallas::Scalar::zero() }
    }
}

/// Trait for secret keys that implements a signature creation
pub trait SchnorrSecret {
    /// Sign a given message, using `rng` as source of randomness.
    fn sign(&self, rng: &mut (impl CryptoRng + RngCore), message: &[u8]) -> Signature;
}

/// Trait for public keys that implements a signature verification
pub trait SchnorrPublic {
    /// Verify a given message is valid given a signature.
    fn verify(&self, message: &[u8], signature: &Signature) -> bool;
}

/// Schnorr signature trait implementations for the stuff in `keypair.rs`
impl SchnorrSecret for SecretKey {
    fn sign(&self, rng: &mut (impl CryptoRng + RngCore), message: &[u8]) -> Signature {
        let mask = pallas::Scalar::random(rng);
        let commit = NullifierK.generator() * mask;

        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, &commit.to_bytes(), message);
        let response = mask + challenge * mod_r_p(self.inner());

        Signature { commit, response }
    }
}

impl SchnorrPublic for PublicKey {
    fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, &signature.commit.to_bytes(), message);
        NullifierK.generator() * signature.response - self.inner() * challenge == signature.commit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkfi_serial::{deserialize, serialize};
    use rand::rngs::OsRng;

    #[test]
    fn test_schnorr_signature() {
        let secret = SecretKey::random(&mut OsRng);
        let message: &[u8] = b"aaaahhhh i'm signiiinngg";
        let signature = secret.sign(&mut OsRng, message);
        let public = PublicKey::from_secret(secret);
        assert!(public.verify(message, &signature));

        // Check out if it's also fine with serialization
        let ser = serialize(&signature);
        let de = deserialize(&ser).unwrap();
        assert!(public.verify(message, &de));
    }
}
