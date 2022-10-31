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

use darkfi_sdk::crypto::constants::{NullifierK, DRK_SCHNORR_DOMAIN};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use halo2_gadgets::ecc::chip::FixedPoint;
use pasta_curves::{
    group::{ff::Field, Group, GroupEncoding},
    pallas,
};
use rand::rngs::OsRng;

use crate::crypto::{
    keypair::{PublicKey, SecretKey},
    util::{hash_to_scalar, mod_r_p},
};

#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Signature {
    commit: pallas::Point,
    response: pallas::Scalar,
}

impl Signature {
    pub fn dummy() -> Self {
        Self { commit: pallas::Point::identity(), response: pallas::Scalar::zero() }
    }
}

pub trait SchnorrSecret {
    fn sign(&self, message: &[u8]) -> Signature;
}

pub trait SchnorrPublic {
    fn verify(&self, message: &[u8], signature: &Signature) -> bool;
}

impl SchnorrSecret for SecretKey {
    fn sign(&self, message: &[u8]) -> Signature {
        let mask = pallas::Scalar::random(&mut OsRng);
        let commit = NullifierK.generator() * mask;

        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, &commit.to_bytes(), message);
        let response = mask + challenge * mod_r_p(self.inner());

        Signature { commit, response }
    }
}

impl SchnorrPublic for PublicKey {
    fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, &signature.commit.to_bytes(), message);
        NullifierK.generator() * signature.response - self.0 * challenge == signature.commit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use darkfi_serial::{deserialize, serialize};

    #[test]
    fn test_schnorr() {
        let secret = SecretKey::random(&mut OsRng);
        let message = b"Foo bar";
        let signature = secret.sign(&message[..]);
        let public = PublicKey::from_secret(secret);
        assert!(public.verify(&message[..], &signature));

        let ser = serialize(&signature);
        let de = deserialize(&ser).unwrap();
        assert!(public.verify(&message[..], &de));
    }
}
