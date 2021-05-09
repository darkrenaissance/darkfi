use ff::Field;
use group::{Group, GroupEncoding};
use rand::rngs::OsRng;
use std::io;

use crate::error::{Error, Result};
use crate::serial::{Decodable, Encodable};
use super::util::hash_to_scalar;

pub struct SecretKey(pub jubjub::Fr);

impl SecretKey {
    pub fn random() -> Self {
        Self(jubjub::Fr::random(&mut OsRng))
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        let mask = jubjub::Fr::random(&mut OsRng);
        let commit = zcash_primitives::constants::SPENDING_KEY_GENERATOR * mask;

        let challenge = hash_to_scalar(b"DarkFi_Schnorr", &commit.to_bytes(), message);

        let response = mask + challenge * self.0;

        Signature { commit, response }
    }

    pub fn public_key(&self) -> PublicKey {
        let public = zcash_primitives::constants::SPENDING_KEY_GENERATOR * self.0;
        PublicKey(public)
    }
}

pub struct PublicKey(pub jubjub::SubgroupPoint);

pub struct Signature {
    commit: jubjub::SubgroupPoint,
    response: jubjub::Fr,
}

impl Encodable for Signature {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        let mut len = 0;
        len += self.commit.encode(&mut s)?;
        len += self.response.encode(s)?;
        Ok(len)
    }
}

impl Decodable for Signature {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self {
            commit: Decodable::decode(&mut d)?,
            response: Decodable::decode(d)?,
        })
    }
}

impl PublicKey {
    pub fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        let challenge = hash_to_scalar(b"DarkFi_Schnorr", &signature.commit.to_bytes(), message);
        zcash_primitives::constants::SPENDING_KEY_GENERATOR * signature.response
            - self.0 * challenge
            == signature.commit
    }
}

#[test]
fn test_schnorr() {
    let secret = SecretKey::random();
    let message = b"Foo bar";
    let signature = secret.sign(&message[..]);
    let public = secret.public_key();
    assert!(public.verify(&message[..], &signature));
}
