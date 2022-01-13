use std::{convert::TryFrom, io};

use halo2_gadgets::ecc::FixedPoints;
use pasta_curves::{
    arithmetic::{Field, FieldExt},
    group::{Group, GroupEncoding},
    pallas,
};
use rand::RngCore;

use crate::{
    crypto::{address::Address, constants::OrchardFixedBases, util::mod_r_p},
    util::serial::{Decodable, Encodable, ReadExt, WriteExt},
    Error, Result,
};

#[derive(Copy, Clone, PartialEq, Debug)]
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

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct SecretKey(pub pallas::Base);

impl SecretKey {
    pub fn random(mut rng: impl RngCore) -> Self {
        let x = pallas::Base::random(&mut rng);
        Self(x)
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_bytes()
    }

    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self> {
        match pallas::Base::from_bytes(bytes).into() {
            Some(k) => Ok(Self(k)),
            None => Err(Error::SecretKeyFromBytes),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct PublicKey(pub pallas::Point);

impl PublicKey {
    pub fn random(mut rng: impl RngCore) -> Self {
        let p = pallas::Point::random(&mut rng);
        Self(p)
    }

    pub fn from_secret(s: SecretKey) -> Self {
        let p = OrchardFixedBases::NullifierK.generator() * mod_r_p(s.0);
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
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(&self.to_bytes()[..])?;
        Ok(32)
    }
}

impl Decodable for pallas::Base {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        let result = pallas::Base::from_bytes(&bytes);
        if result.is_some().into() {
            Ok(result.unwrap())
        } else {
            Err(Error::BadOperationType)
        }
    }
}

impl Encodable for pallas::Scalar {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(&self.to_bytes()[..])?;
        Ok(32)
    }
}

impl Decodable for pallas::Scalar {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        let result = pallas::Scalar::from_bytes(&bytes);
        if result.is_some().into() {
            Ok(result.unwrap())
        } else {
            Err(Error::BadOperationType)
        }
    }
}

impl Encodable for pallas::Point {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(&self.to_bytes()[..])?;
        Ok(32)
    }
}

impl Decodable for pallas::Point {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        let result = Self::from_bytes(&bytes);
        if result.is_some().into() {
            Ok(result.unwrap())
        } else {
            Err(Error::BadOperationType)
        }
    }
}

impl Encodable for SecretKey {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(&self.0.to_bytes()[..])?;
        Ok(32)
    }
}

impl Decodable for SecretKey {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        let result = pallas::Base::from_bytes(&bytes);
        if result.is_some().into() {
            Ok(SecretKey(result.unwrap()))
        } else {
            Err(Error::BadOperationType)
        }
    }
}

impl Encodable for PublicKey {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(&self.0.to_bytes()[..])?;
        Ok(32)
    }
}

impl Decodable for PublicKey {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; 32];
        d.read_slice(&mut bytes)?;
        let result = pallas::Point::from_bytes(&bytes);
        if result.is_some().into() {
            Ok(PublicKey(result.unwrap()))
        } else {
            log::debug!("Failed decoding PublicKey");
            Err(Error::BadOperationType)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        crypto::util::pedersen_commitment_scalar,
        util::serial::{deserialize, serialize},
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

        let a = pallas::Scalar::from(420);
        let b = pallas::Scalar::from(69);
        let pc: pallas::Point = pedersen_commitment_scalar(a, b);
        let serialized = serialize(&pc);
        assert_eq!(
            serialized,
            vec![
                55, 48, 126, 42, 114, 27, 18, 55, 155, 141, 83, 75, 44, 50, 244, 223, 254, 216, 22,
                167, 208, 59, 212, 201, 150, 149, 96, 207, 216, 74, 60, 131
            ]
        );
        assert_eq!(deserialize(&serialized).ok(), Some(pc));

        Ok(())
    }
}
