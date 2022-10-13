use std::{convert::TryFrom, io, str::FromStr};

use halo2_gadgets::ecc::chip::FixedPoint;
use pasta_curves::{
    arithmetic::CurveAffine,
    group::{
        ff::{Field, PrimeField},
        Curve, Group, GroupEncoding,
    },
    pallas,
};
use rand::RngCore;

use crate::{
    crypto::{address::Address, constants::NullifierK, util::mod_r_p},
    serial::{Decodable, Encodable, ReadExt, SerialDecodable, SerialEncodable, WriteExt},
    Error, Result,
};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[cfg(feature = "serde")]
#[derive(serde::Deserialize, serde::Serialize)]
pub struct Keypair {
    pub secret: SecretKey,
    pub public: PublicKey,
}

impl Keypair {
    pub fn new(secret: SecretKey) -> Self {
        let public = PublicKey::from_secret(secret);
        Self { secret, public }
    }

    pub fn random(mut rng: impl RngCore) -> Self {
        let secret = SecretKey::random(&mut rng);
        Self::new(secret)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, SerialDecodable, SerialEncodable)]
pub struct SecretKey(pallas::Base);

impl SecretKey {
    pub fn random(mut rng: impl RngCore) -> Self {
        let x = pallas::Base::random(&mut rng);
        Self(x)
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_repr()
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self> {
        match pallas::Base::from_repr(bytes).into() {
            Some(k) => Ok(Self(k)),
            None => Err(Error::SecretKeyFromBytes),
        }
    }

    pub fn inner(&self) -> pallas::Base {
        self.0
    }
}

impl From<pallas::Base> for SecretKey {
    fn from(x: pallas::Base) -> Self {
        Self(x)
    }
}

impl FromStr for SecretKey {
    type Err = crate::Error;

    /// Tries to create a `SecretKey` instance from a base58 encoded string.
    fn from_str(encoded: &str) -> core::result::Result<Self, crate::Error> {
        let decoded = bs58::decode(encoded).into_vec()?;
        if decoded.len() != 32 {
            return Err(Error::SecretKeyFromStr)
        }
        Self::from_bytes(decoded.try_into().unwrap())
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, SerialDecodable, SerialEncodable)]
pub struct PublicKey(pub pallas::Point);

impl PublicKey {
    pub fn random(mut rng: impl RngCore) -> Self {
        let p = pallas::Point::random(&mut rng);
        Self(p)
    }

    pub fn from_secret(s: SecretKey) -> Self {
        let nfk = NullifierK;
        let p = nfk.generator() * mod_r_p(s.0);
        Self(p)
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_bytes()
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self> {
        match pallas::Point::from_bytes(bytes).into() {
            Some(k) => Ok(Self(k)),
            None => Err(Error::PublicKeyFromBytes),
        }
    }

    pub fn x(&self) -> pallas::Base {
        *self.0.to_affine().coordinates().unwrap().x()
    }

    pub fn y(&self) -> pallas::Base {
        *self.0.to_affine().coordinates().unwrap().y()
    }
}

impl FromStr for PublicKey {
    type Err = crate::Error;

    /// Tries to create a `PublicKey` instance from a base58 encoded string.
    fn from_str(encoded: &str) -> core::result::Result<Self, crate::Error> {
        let decoded = bs58::decode(encoded).into_vec()?;
        if decoded.len() != 32 {
            return Err(Error::PublicKeyFromStr)
        }

        Self::from_bytes(&decoded.try_into().unwrap())
    }
}

impl TryFrom<Address> for PublicKey {
    type Error = Error;
    fn try_from(address: Address) -> Result<Self> {
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&address.0[1..33]);
        Self::from_bytes(&bytes)
    }
}

impl Encodable for pallas::Base {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        s.write_slice(&self.to_repr()[..])?;
        Ok(32)
    }
}

impl Decodable for pallas::Base {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        let result = pallas::Base::from_repr(bytes);
        if result.is_some().into() {
            Ok(result.unwrap())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "Failed to deserialize pallas::Base"))
        }
    }
}

impl Encodable for pallas::Scalar {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        s.write_slice(&self.to_repr()[..])?;
        Ok(32)
    }
}

impl Decodable for pallas::Scalar {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        let result = pallas::Scalar::from_repr(bytes);
        if result.is_some().into() {
            Ok(result.unwrap())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "Failed to deserialize pallas::Scalar"))
        }
    }
}

impl Encodable for pallas::Point {
    fn encode<S: io::Write>(&self, mut s: S) -> core::result::Result<usize, io::Error> {
        s.write_slice(&self.to_bytes()[..])?;
        Ok(32)
    }
}

impl Decodable for pallas::Point {
    fn decode<D: io::Read>(mut d: D) -> core::result::Result<Self, io::Error> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        let result = Self::from_bytes(&bytes);
        if result.is_some().into() {
            Ok(result.unwrap())
        } else {
            Err(io::Error::new(io::ErrorKind::Other, "Failed to deserialize pallas::Point"))
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for SecretKey {
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
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
struct SecretKeyVisitor;

#[cfg(feature = "serde")]
impl<'de> serde::de::Visitor<'de> for SecretKeyVisitor {
    type Value = SecretKey;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("hex string")
    }

    fn visit_str<E>(self, value: &str) -> core::result::Result<SecretKey, E>
    where
        E: serde::de::Error,
    {
        let bytes = hex::decode(value).unwrap();
        let mut r = std::io::Cursor::new(bytes);
        let decoded: SecretKey = SecretKey::decode(&mut r).unwrap();
        Ok(decoded)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SecretKey {
    fn deserialize<D>(deserializer: D) -> core::result::Result<SecretKey, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = deserializer.deserialize_str(SecretKeyVisitor).unwrap();
        Ok(bytes)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
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
struct PublicKeyVisitor;

#[cfg(feature = "serde")]
impl<'de> serde::de::Visitor<'de> for PublicKeyVisitor {
    type Value = PublicKey;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("hex string")
    }

    fn visit_str<E>(self, value: &str) -> core::result::Result<PublicKey, E>
    where
        E: serde::de::Error,
    {
        let bytes = hex::decode(value).unwrap();
        let mut r = std::io::Cursor::new(bytes);
        let decoded: PublicKey = PublicKey::decode(&mut r).unwrap();
        Ok(decoded)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> core::result::Result<PublicKey, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = deserializer.deserialize_str(PublicKeyVisitor).unwrap();
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::util::pedersen_commitment_base,
        serial::{deserialize, serialize},
    };

    #[test]
    fn test_pasta_serialization() -> Result<()> {
        let fifty_five = pallas::Base::from(55);
        let serialized = serialize(&fifty_five);
        assert_eq!(
            serialized,
            vec![
                55, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0
            ]
        );
        assert_eq!(deserialize(&serialized).ok(), Some(fifty_five));

        let fourtwenty = pallas::Scalar::from(42069);
        let serialized = serialize(&fourtwenty);
        assert_eq!(
            serialized,
            vec![
                85, 164, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0
            ]
        );
        assert_eq!(deserialize(&serialized).ok(), Some(fourtwenty));

        let a = pallas::Base::from(420);
        let b = pallas::Scalar::from(69);
        let pc: pallas::Point = pedersen_commitment_base(a, b);
        let serialized = serialize(&pc);
        assert_eq!(
            serialized,
            vec![
                57, 232, 32, 239, 229, 119, 41, 70, 218, 174, 237, 25, 122, 81, 81, 252, 54, 192,
                225, 207, 145, 124, 177, 46, 28, 37, 55, 70, 6, 33, 51, 42,
            ]
        );
        assert_eq!(deserialize(&serialized).ok(), Some(pc));

        Ok(())
    }
}
