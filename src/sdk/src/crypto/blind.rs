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

#[cfg(feature = "async")]
use darkfi_serial::{async_trait, AsyncDecodable, AsyncEncodable};
use darkfi_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};

use pasta_curves::{group::ff::Field, pallas};

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

    pub fn random<RngCore: rand_core::RngCore>(rng: &mut RngCore) -> Self {
        Self(F::random(rng))
    }

    pub fn inner(&self) -> F {
        self.0
    }
}

impl<'a, 'b, F: Field + EncDecode> std::ops::Add<&'b Blind<F>> for &'a Blind<F> {
    type Output = Blind<F>;

    #[inline]
    fn add(self, rhs: &'b Blind<F>) -> Blind<F> {
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

impl From<u64> for ScalarBlind {
    fn from(x: u64) -> Self {
        Self(pallas::Scalar::from(x))
    }
}
