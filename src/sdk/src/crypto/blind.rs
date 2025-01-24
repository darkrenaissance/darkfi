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
use darkfi_serial::{async_trait, AsyncDecodable, AsyncEncodable};
use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};

use pasta_curves::{
    group::ff::{Field, PrimeField},
    pallas,
};
use rand_core::{CryptoRng, RngCore};

use crate::error::ContractError;

#[cfg(feature = "async")]
pub trait EncDecode: Encodable + Decodable + AsyncEncodable + AsyncDecodable {}
#[cfg(not(feature = "async"))]
pub trait EncDecode: Encodable + Decodable {}

impl EncDecode for pallas::Base {}
impl EncDecode for pallas::Scalar {}

/// Blinding factor used in bullas. Every bulla should contain one.
#[derive(Debug, Copy, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Blind<F: Field + EncDecode>(pub F);

impl<F: Field + EncDecode> Blind<F> {
    pub const ZERO: Self = Self(F::ZERO);

    pub fn random(rng: &mut (impl CryptoRng + RngCore)) -> Self {
        Self(F::random(rng))
    }

    pub fn inner(&self) -> F {
        self.0
    }
}

impl<'a, F: Field + EncDecode> std::ops::Add<&'a Blind<F>> for &Blind<F> {
    type Output = Blind<F>;

    #[inline]
    fn add(self, rhs: &'a Blind<F>) -> Blind<F> {
        Blind(self.0.add(rhs.0))
    }
}

impl<F: Field + EncDecode> std::ops::AddAssign for Blind<F> {
    #[inline]
    fn add_assign(&mut self, other: Self) {
        self.0.add_assign(other.0)
    }
}

pub type BaseBlind = Blind<pallas::Base>;
pub type ScalarBlind = Blind<pallas::Scalar>;

impl From<u64> for BaseBlind {
    fn from(x: u64) -> Self {
        Self(pallas::Base::from(x))
    }
}

impl FromStr for BaseBlind {
    type Err = ContractError;

    /// Tries to create a `BaseBlind` object from a base58 encoded string.
    fn from_str(enc: &str) -> Result<Self, Self::Err> {
        let decoded = bs58::decode(enc).into_vec()?;
        if decoded.len() != 32 {
            return Err(Self::Err::IoError(
                "Failed decoding BaseBlind from bytes, len is not 32".to_string(),
            ))
        }

        match pallas::Base::from_repr(decoded.try_into().unwrap()).into() {
            Some(k) => Ok(Self(k)),
            None => Err(ContractError::IoError("Could not convert bytes to BaseBlind".to_string())),
        }
    }
}

impl core::fmt::Display for BaseBlind {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let disp: String = bs58::encode(self.0.to_repr()).into_string();
        write!(f, "{}", disp)
    }
}

impl From<u64> for ScalarBlind {
    fn from(x: u64) -> Self {
        Self(pallas::Scalar::from(x))
    }
}

impl FromStr for ScalarBlind {
    type Err = ContractError;

    /// Tries to create a `ScalarBlind` object from a base58 encoded string.
    fn from_str(enc: &str) -> Result<Self, Self::Err> {
        let decoded = bs58::decode(enc).into_vec()?;
        if decoded.len() != 32 {
            return Err(Self::Err::IoError(
                "Failed decoding ScalarBlind from bytes, len is not 32".to_string(),
            ))
        }

        match pallas::Scalar::from_repr(decoded.try_into().unwrap()).into() {
            Some(k) => Ok(Self(k)),
            None => {
                Err(ContractError::IoError("Could not convert bytes to ScalarBlind".to_string()))
            }
        }
    }
}

impl core::fmt::Display for ScalarBlind {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        let disp: String = bs58::encode(self.0.to_repr()).into_string();
        write!(f, "{}", disp)
    }
}
