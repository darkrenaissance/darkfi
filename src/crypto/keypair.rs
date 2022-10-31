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

use std::{
    convert::TryFrom,
    hash::{Hash, Hasher},
    str::FromStr,
};

use darkfi_sdk::crypto::constants::NullifierK;
use darkfi_serial::{SerialDecodable, SerialEncodable};
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
    crypto::{address::Address, util::mod_r_p},
    Error, Result,
};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
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
pub struct SecretKey(pub pallas::Base);

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

    pub fn xy(&self) -> (pallas::Base, pallas::Base) {
        let coords = self.0.to_affine().coordinates().unwrap();
        (*coords.x(), *coords.y())
    }
}

impl Hash for PublicKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let bytes = self.0.to_affine().to_bytes();
        bytes.hash(state);
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
