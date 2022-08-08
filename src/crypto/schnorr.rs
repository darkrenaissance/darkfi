use std::io;

use halo2_gadgets::ecc::chip::FixedPoint;
use pasta_curves::{
    group::{ff::Field, GroupEncoding},
    pallas,
};
use rand::rngs::OsRng;

use crate::{
    crypto::{
        constants::{NullifierK, DRK_SCHNORR_DOMAIN},
        keypair::{PublicKey, SecretKey},
        util::{hash_to_scalar, mod_r_p},
    },
    util::serial::{Decodable, Encodable},
    Result,
};

#[derive(Debug, Clone, PartialEq)]
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
        let commit = NullifierK.generator() * mask;

        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, &commit.to_bytes(), message);
        let response = mask + challenge * mod_r_p(self.0);

        Signature { commit, response }
    }
}

impl SchnorrPublic for PublicKey {
    fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        let challenge = hash_to_scalar(DRK_SCHNORR_DOMAIN, &signature.commit.to_bytes(), message);
        NullifierK.generator() * signature.response - self.0 * challenge == signature.commit
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

#[cfg(feature = "serde")]
impl serde::Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut bytes = vec![];
        self.encode(&mut bytes).unwrap();
        let hex_repr = hex::encode(&bytes);
        serializer.serialize_str(&hex_repr)
    }
}

#[cfg(feature = "serde")]
struct SignatureVisitor;

#[cfg(feature = "serde")]
impl<'de> serde::de::Visitor<'de> for SignatureVisitor {
    type Value = Signature;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("hex string")
    }

    fn visit_str<E>(self, value: &str) -> std::result::Result<Signature, E>
    where
        E: serde::de::Error,
    {
        let bytes = hex::decode(value).unwrap();
        let mut r = std::io::Cursor::new(bytes);
        let decoded: Signature = Signature::decode(&mut r).unwrap();
        Ok(decoded)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Signature, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = deserializer.deserialize_str(SignatureVisitor).unwrap();
        Ok(bytes)
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
