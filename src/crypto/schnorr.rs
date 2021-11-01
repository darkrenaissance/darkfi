use std::io;

use halo2_gadgets::ecc::FixedPoints;
use pasta_curves as pasta;
use pasta_curves::{arithmetic::Field, group::GroupEncoding};
use rand::rngs::OsRng;

use super::{
    constants::{OrchardFixedBases, DRK_SCHNORR_DOMAIN},
    util::hash_to_scalar,
};
use crate::error::Result;
use crate::serial::{Decodable, Encodable};

pub struct SecretKey(pub pasta::Fq);

impl SecretKey {
    pub fn random() -> Self {
        Self(pasta::Fq::random(&mut OsRng))
    }

    pub fn sign(&self, message: &[u8]) -> Signature {
        let mask = pasta::Fq::random(&mut OsRng);
        let commit = OrchardFixedBases::SpendAuthG.generator() * mask;

        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, &commit.to_bytes(), message);
        let response = mask + challenge * self.0;

        Signature { commit, response }
    }

    pub fn public_key(&self) -> PublicKey {
        let public = OrchardFixedBases::SpendAuthG.generator() * self.0;
        PublicKey(public)
    }
}

pub struct PublicKey(pub pasta::Ep);

pub struct Signature {
    commit: pasta::Ep,
    response: pasta::Fq,
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
        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, &signature.commit.to_bytes(), message);
        OrchardFixedBases::SpendAuthG.generator() * signature.response - self.0 * challenge
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
