use std::io;

use halo2_gadgets::ecc::FixedPoints;
use pasta_curves::{
    arithmetic::{Field, FieldExt},
    group::{Group, GroupEncoding},
    pallas,
};
use rand::RngCore;

use crate::{
    crypto::{constants::OrchardFixedBases, util::mod_r_p},
    serial::{Decodable, Encodable, ReadExt, WriteExt},
    Error, Result,
};

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Keypair {
    pub secret: SecretKey,
    pub public: PublicKey,
}

impl Keypair {
    pub fn new(secret: SecretKey) -> Self {
        let public = PublicKey::from_secret(secret.clone());
        Keypair { secret, public }
    }

    pub fn random(mut rng: impl RngCore) -> Self {
        let secret = SecretKey::random(&mut rng);
        Keypair::new(secret)
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct SecretKey(pub pallas::Base);

impl SecretKey {
    pub fn random(mut rng: impl RngCore) -> Self {
        let x = pallas::Base::random(&mut rng);
        SecretKey(x)
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_bytes()
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct PublicKey(pub pallas::Point);

impl PublicKey {
    pub fn random(mut rng: impl RngCore) -> Self {
        let p = pallas::Point::random(&mut rng);
        PublicKey(p)
    }

    pub fn from_secret(s: SecretKey) -> Self {
        let p = OrchardFixedBases::NullifierK.generator() * mod_r_p(s.0);
        PublicKey(p)
    }

    pub fn to_bytes(self) -> [u8; 32] {
        self.0.to_bytes()
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
            Err(Error::BadOperationType)
        }
    }
}
