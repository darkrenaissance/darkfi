/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2024 Dyne.org foundation
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

use core::str::FromStr;

#[cfg(feature = "async")]
use darkfi_serial::async_trait;
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
use rand_core::{CryptoRng, RngCore};

use super::{constants::NullifierK, util::fp_mod_fv};
use crate::error::ContractError;

/// Keypair structure holding a `SecretKey` and its respective `PublicKey`
#[derive(Copy, Clone, PartialEq, Eq, Debug, SerialEncodable, SerialDecodable)]
pub struct Keypair {
    pub secret: SecretKey,
    pub public: PublicKey,
}

impl Keypair {
    /// Instantiate a new `Keypair` given a `SecretKey`
    pub fn new(secret: SecretKey) -> Self {
        Self { secret, public: PublicKey::from_secret(secret) }
    }

    /// Generate a new `Keypair` object given a source of randomness
    pub fn random(rng: &mut (impl CryptoRng + RngCore)) -> Self {
        Self::new(SecretKey::random(rng))
    }
}

impl Default for Keypair {
    /// Default Keypair used in genesis block generation
    fn default() -> Self {
        let secret = SecretKey::from(pallas::Base::from(42));
        let public = PublicKey::from_secret(secret);
        Self { secret, public }
    }
}

/// Structure holding a secret key, wrapping a `pallas::Base` element.
#[derive(Copy, Clone, PartialEq, Eq, Debug, SerialEncodable, SerialDecodable)]
pub struct SecretKey(pallas::Base);

impl SecretKey {
    /// Get the inner object wrapped by `SecretKey`
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Generate a new `SecretKey` given a source of randomness
    pub fn random(rng: &mut (impl CryptoRng + RngCore)) -> Self {
        Self(pallas::Base::random(rng))
    }

    /// Instantiate a `SecretKey` given 32 bytes. Returns an error
    /// if the representation is noncanonical.
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(bytes).into() {
            Some(k) => Ok(Self(k)),
            None => Err(ContractError::IoError("Could not convert bytes to SecretKey".to_string())),
        }
    }
}

impl From<pallas::Base> for SecretKey {
    fn from(x: pallas::Base) -> Self {
        Self(x)
    }
}

impl FromStr for SecretKey {
    type Err = ContractError;

    /// Tries to create a `SecretKey` object from a base58 encoded string.
    fn from_str(enc: &str) -> Result<Self, Self::Err> {
        let decoded = bs58::decode(enc).into_vec()?;
        if decoded.len() != 32 {
            return Err(Self::Err::IoError(
                "Failed decoding SecretKey from bytes, len is not 32".to_string(),
            ))
        }

        Self::from_bytes(decoded.try_into().unwrap())
    }
}

impl core::fmt::Display for SecretKey {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let disp: String = bs58::encode(self.0.to_repr()).into_string();
        write!(f, "{}", disp)
    }
}

/// Structure holding a public key, wrapping a `pallas::Point` element.
#[derive(Copy, Clone, PartialEq, Eq, Debug, SerialEncodable, SerialDecodable)]
pub struct PublicKey(pallas::Point);

impl PublicKey {
    /// Get the inner object wrapped by `PublicKey`
    pub fn inner(&self) -> pallas::Point {
        self.0
    }

    /// Derive a new `PublicKey` object given a `SecretKey`
    pub fn from_secret(s: SecretKey) -> Self {
        let p = NullifierK.generator() * fp_mod_fv(s.inner());
        Self(p)
    }

    /// Instantiate a `PublicKey` given 32 bytes. Returns an error
    /// if the representation is noncanonical.
    pub fn from_bytes(bytes: [u8; 32]) -> Result<Self, ContractError> {
        match <subtle::CtOption<pallas::Point> as Into<Option<pallas::Point>>>::into(
            pallas::Point::from_bytes(&bytes),
        ) {
            Some(k) => {
                if bool::from(k.is_identity()) {
                    return Err(ContractError::IoError(
                        "Could not convert bytes to PublicKey".to_string(),
                    ))
                }

                Ok(Self(k))
            }
            None => Err(ContractError::IoError("Could not convert bytes to PublicKey".to_string())),
        }
    }

    /// Downcast the `PublicKey` to 32 bytes of `pallas::Point`
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_bytes()
    }

    /// Fetch the `x` coordinate of this `PublicKey`
    pub fn x(&self) -> pallas::Base {
        *self.0.to_affine().coordinates().unwrap().x()
    }

    /// Fetch the `y` coordinate of this `PublicKey`
    pub fn y(&self) -> pallas::Base {
        *self.0.to_affine().coordinates().unwrap().y()
    }

    /// Fetch the `x` and `y` coordinates of this `PublicKey` as a tuple
    pub fn xy(&self) -> (pallas::Base, pallas::Base) {
        let coords = self.0.to_affine().coordinates().unwrap();
        (*coords.x(), *coords.y())
    }
}

impl TryFrom<pallas::Point> for PublicKey {
    type Error = ContractError;

    fn try_from(x: pallas::Point) -> Result<Self, Self::Error> {
        if bool::from(x.is_identity()) {
            return Err(ContractError::IoError(
                "Could not convert identity point to PublicKey".to_string(),
            ))
        }

        Ok(Self(x))
    }
}

impl FromStr for PublicKey {
    type Err = ContractError;

    /// Tries to create a `PublicKey` object from a base58 encoded string.
    fn from_str(enc: &str) -> Result<Self, Self::Err> {
        let decoded = bs58::decode(enc).into_vec()?;
        if decoded.len() != 32 {
            return Err(Self::Err::IoError(
                "Failed decoding PublicKey from bytes, len is not 32".to_string(),
            ))
        }

        Self::from_bytes(decoded.try_into().unwrap())
    }
}

impl core::fmt::Display for PublicKey {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let disp: String = bs58::encode(self.0.to_bytes()).into_string();
        write!(f, "{}", disp)
    }
}
