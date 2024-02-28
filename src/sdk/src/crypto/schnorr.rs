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
    group::{ff::PrimeField, Group, GroupEncoding},
    pallas,
};

use super::{
    constants::{NullifierK, DRK_SCHNORR_DOMAIN},
    util::{fp_mod_fv, hash_to_scalar},
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
    /// Sign a given message
    fn sign(&self, message: &[u8]) -> Signature;
}

/// Trait for public keys that implements a signature verification
pub trait SchnorrPublic {
    /// Verify a given message is valid given a signature.
    fn verify(&self, message: &[u8], signature: &Signature) -> bool;
}

/// Schnorr signature trait implementations for the stuff in `keypair.rs`
impl SchnorrSecret for SecretKey {
    fn sign(&self, message: &[u8]) -> Signature {
        // Derive a deterministic nonce
        let mask = hash_to_scalar(DRK_SCHNORR_DOMAIN, &[&self.inner().to_repr(), message]);

        let commit = NullifierK.generator() * mask;

        let commit_bytes = commit.to_bytes();
        let pubkey_bytes = PublicKey::from_secret(*self).to_bytes();
        let transcript = &[&commit_bytes, &pubkey_bytes, message];

        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, transcript);
        let response = mask + challenge * fp_mod_fv(self.inner());

        Signature { commit, response }
    }
}

impl SchnorrPublic for PublicKey {
    fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        let commit_bytes = signature.commit.to_bytes();
        let pubkey_bytes = self.to_bytes();
        let transcript = &[&commit_bytes, &pubkey_bytes, message];

        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, transcript);
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
        let signature = secret.sign(message);
        let public = PublicKey::from_secret(secret);
        assert!(public.verify(message, &signature));

        // Check out if it's also fine with serialization
        let ser = serialize(&signature);
        let de = deserialize(&ser).unwrap();
        assert!(public.verify(message, &de));
    }
}
