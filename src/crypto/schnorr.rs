use std::io;

use halo2_gadgets::ecc::FixedPoints;
use pasta_curves::{arithmetic::Field, group::GroupEncoding, pallas};
use rand::rngs::OsRng;

use crate::{
    crypto::{
        constants::{OrchardFixedBases, DRK_SCHNORR_DOMAIN},
        keypair::{PublicKey, SecretKey},
        util::{hash_to_scalar, mod_r_p},
    },
    serial::{Decodable, Encodable},
    Result,
};

#[derive(Debug)]
pub struct Signature {
    commit: pallas::Point,
    response: pallas::Scalar,
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
        let commit = OrchardFixedBases::NullifierK.generator() * mask;

        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, &commit.to_bytes(), message);
        let response = mask + challenge * mod_r_p(self.0);

        Signature { commit, response }
    }
}

impl SchnorrPublic for PublicKey {
    fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, &signature.commit.to_bytes(), message);
        OrchardFixedBases::NullifierK.generator() * signature.response - self.0 * challenge ==
            signature.commit
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schnorr() {
        let secret = SecretKey::random(&mut OsRng);
        let message = b"Foo bar";
        let signature = secret.sign(&message[..]);
        let public = PublicKey::from_secret(secret);
        assert!(public.verify(&message[..], &signature));
    }
}
