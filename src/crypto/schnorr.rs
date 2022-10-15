use darkfi_serial::{SerialDecodable, SerialEncodable};
use halo2_gadgets::ecc::chip::FixedPoint;
use pasta_curves::{
    group::{ff::Field, Group, GroupEncoding},
    pallas,
};
use rand::rngs::OsRng;

use crate::crypto::{
    constants::{NullifierK, DRK_SCHNORR_DOMAIN},
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
