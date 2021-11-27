use std::io;

use halo2_gadgets::ecc::FixedPoints;
use pasta_curves::{arithmetic::Field, group::GroupEncoding, pallas};
use rand::rngs::OsRng;

use super::{
    constants::{OrchardFixedBases, DRK_SCHNORR_DOMAIN},
    util::{hash_to_scalar, mod_r_p},
};
use crate::{
    error::Result,
    serial::{Decodable, Encodable},
    types::{
        derive_public_key, DrkCoinBlind, DrkPublicKey, DrkSecretKey, DrkSerial, DrkTokenId,
        DrkValueBlind, DrkValueCommit,
    },
};

#[derive(Clone)]
pub struct SecretKey(pub pallas::Scalar);

impl SecretKey {
    pub fn random() -> Self {
        Self(pallas::Scalar::random(&mut OsRng))
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        let mask = DrkValueBlind::random(&mut OsRng);
        let commit = OrchardFixedBases::SpendAuthG.generator() * mask;

        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, &commit.to_bytes(), message);
        let response = mask + challenge * self.0;

        Signature { commit, response }
    }

    pub fn public_key(&self) -> PublicKey {
        let public_key = OrchardFixedBases::SpendAuthG.generator() * self.0;
        PublicKey(public_key)
    }
}

#[derive(PartialEq)]
pub struct PublicKey(pub DrkPublicKey);

pub struct Signature {
    commit: DrkValueCommit,
    response: DrkValueBlind,
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
        Ok(Self { commit: Decodable::decode(&mut d)?, response: Decodable::decode(d)? })
    }
}

impl PublicKey {
    pub fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, &signature.commit.to_bytes(), message);
        OrchardFixedBases::SpendAuthG.generator() * signature.response - self.0 * challenge ==
            signature.commit
    }
}

impl Encodable for PublicKey {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        Ok(self.0.encode(s)?)
    }
}

impl Decodable for PublicKey {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        Ok(Self(Decodable::decode(&mut d)?))
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
