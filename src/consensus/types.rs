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

//! Type aliases used in the consensus codebase.
use std::ops::{Add, AddAssign, Div, Mul, Sub};

use dashu::{
    base::Abs,
    float::{round::mode::Zero, FBig, Repr},
};

use super::constants::RADIX_BITS;

const B: u64 = 10;

#[derive(Clone, PartialEq, PartialOrd, Debug)]
pub struct Float10(FBig<Zero, B>);

impl Float10 {
    pub fn repr(&self) -> &Repr<B> {
        self.0.repr()
    }

    pub fn abs(&self) -> Self {
        Self(self.0.clone().abs())
    }

    pub fn powf(&self, exp: Self) -> Self {
        Self(self.0.powf(&exp.0))
    }

    pub fn ln(&self) -> Self {
        Self(self.0.ln())
    }
}

impl Add for Float10 {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl AddAssign for Float10 {
    fn add_assign(&mut self, other: Self) {
        *self = Self(self.0.clone() + other.0);
    }
}

impl Sub for Float10 {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl Mul for Float10 {
    type Output = Self;

    fn mul(self, other: Self) -> Self {
        Self(self.0 * other.0)
    }
}

impl Div for Float10 {
    type Output = Self;

    fn div(self, other: Self) -> Self {
        Self(self.0 / other.0)
    }
}

impl std::fmt::Display for Float10 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<&str> for Float10 {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(Self(FBig::from_str_native(value)?.with_precision(RADIX_BITS).value()))
    }
}

impl TryFrom<u64> for Float10 {
    type Error = crate::Error;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Ok(Self(FBig::from(value)))
    }
}

impl TryFrom<i64> for Float10 {
    type Error = crate::Error;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Ok(Self(FBig::from(value)))
    }
}
