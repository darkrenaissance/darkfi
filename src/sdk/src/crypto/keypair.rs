/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
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
        write!(f, "{disp}")
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
        write!(f, "{disp}")
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Network {
    Mainnet,
    Testnet,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum AddressPrefix {
    MainnetStandard = 0x63,
    TestnetStandard = 0x87,
}

impl AddressPrefix {
    pub fn network(&self) -> Network {
        match self {
            Self::MainnetStandard => Network::Mainnet,
            Self::TestnetStandard => Network::Testnet,
        }
    }
}

impl TryFrom<u8> for AddressPrefix {
    type Error = ContractError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x63 => Ok(Self::MainnetStandard),
            0x87 => Ok(Self::TestnetStandard),
            _ => Err(ContractError::IoError("Invalid address type".to_string())),
        }
    }
}

/// Defines a standard DarkFi pasta curve address containing spending and
/// viewing pubkeys.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct StandardAddress {
    network: Network,
    spending_key: PublicKey,
    viewing_key: PublicKey,
}

impl StandardAddress {
    pub fn prefix(&self) -> AddressPrefix {
        match self.network {
            Network::Mainnet => AddressPrefix::MainnetStandard,
            Network::Testnet => AddressPrefix::TestnetStandard,
        }
    }
}

impl From<StandardAddress> for Address {
    fn from(v: StandardAddress) -> Self {
        Address::Standard(v)
    }
}

/// Addresses defined on DarkFi. Catch-all enum.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Address {
    Standard(StandardAddress),
}

impl Address {
    pub fn network(&self) -> Network {
        match self {
            Self::Standard(addr) => addr.network,
        }
    }
}

impl FromStr for Address {
    type Err = ContractError;

    fn from_str(enc: &str) -> Result<Self, Self::Err> {
        let dec = bs58::decode(enc).with_check(None).into_vec()?;
        if dec.is_empty() {
            return Err(ContractError::IoError("Empty address".to_string()))
        }

        let r_addrtype = AddressPrefix::try_from(dec[0])?;
        match r_addrtype {
            AddressPrefix::MainnetStandard | AddressPrefix::TestnetStandard => {
                // Standard addresses consist of [prefix][spend_key][view_key][checksum].
                // Prefix is 1 byte, keys are 32 byte each, and checksum is 4 bytes. This
                // should total to 69 bytes for standard addresses.
                if dec.len() != 69 {
                    return Err(Self::Err::IoError("Invalid address length".to_string()))
                }

                let r_spending_key = PublicKey::from_bytes(dec[1..33].try_into().unwrap())?;
                let r_viewing_key = PublicKey::from_bytes(dec[33..65].try_into().unwrap())?;
                let r_checksum = &dec[65..];

                let checksum = blake3::hash(&dec[..65]);
                if r_checksum != &checksum.as_bytes()[..4] {
                    return Err(Self::Err::IoError("Invalid address checksum".to_string()))
                }

                let addr = StandardAddress {
                    network: r_addrtype.network(),
                    spending_key: r_spending_key,
                    viewing_key: r_viewing_key,
                };

                Ok(Self::Standard(addr))
            }
        }
    }
}

impl core::fmt::Display for Address {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let payload = match self {
            Self::Standard(addr) => {
                let mut payload = Vec::with_capacity(69);
                payload.push(addr.prefix() as u8);
                payload.extend_from_slice(&addr.spending_key.to_bytes());
                payload.extend_from_slice(&addr.viewing_key.to_bytes());
                let checksum = blake3::hash(&payload);
                payload.extend_from_slice(&checksum.as_bytes()[..4]);
                payload
            }
        };

        write!(f, "{}", bs58::encode(payload).with_check().into_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::crypto::poseidon_hash;
    use rand::rngs::OsRng;

    #[test]
    fn test_standard_address_encoding() {
        let s_kp = Keypair::random(&mut OsRng);
        let v_kp = Keypair::new(SecretKey::from(poseidon_hash([s_kp.secret.inner()])));

        let s_addr = StandardAddress {
            network: Network::Mainnet,
            spending_key: s_kp.public,
            viewing_key: v_kp.public,
        };

        let addr: Address = s_addr.into();
        let encoded = addr.to_string();
        let decoded = Address::from_str(&encoded).unwrap();

        assert_eq!(addr, decoded);
    }
}
