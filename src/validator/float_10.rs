/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
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

use std::ops::{Add, AddAssign, Div, Mul, Sub};

use darkfi_sdk::pasta::{group::ff::PrimeField, pallas};
use dashu::{
    base::Abs,
    float::{round::mode::Zero, FBig, Repr},
    integer::{IBig, Sign, UBig},
};
use lazy_static::lazy_static;

const RADIX_BITS: usize = 76;
const B: u64 = 10;

/// Wrapper structure over a Base 10 [`dashu::float::FBig`]
/// and Zero rounding mode.
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

    pub fn to_f64(&self) -> f64 {
        self.0.to_f64().value()
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

impl TryFrom<f64> for Float10 {
    type Error = crate::Error;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Ok(Self(FBig::try_from(value)?.with_precision(RADIX_BITS).value().with_base().value()))
    }
}

// Commonly used Float10
lazy_static! {
    pub static ref FLOAT10_NEG_TWO: Float10 = Float10::try_from("-2").unwrap();
    pub static ref FLOAT10_NEG_ONE: Float10 = Float10::try_from("-1").unwrap();
    pub static ref FLOAT10_ZERO: Float10 = Float10::try_from("0").unwrap();
    pub static ref FLOAT10_ONE: Float10 = Float10::try_from("1").unwrap();
    pub static ref FLOAT10_TWO: Float10 = Float10::try_from("2").unwrap();
    pub static ref FLOAT10_THREE: Float10 = Float10::try_from("3").unwrap();
    pub static ref FLOAT10_FIVE: Float10 = Float10::try_from("5").unwrap();
    pub static ref FLOAT10_NINE: Float10 = Float10::try_from("9").unwrap();
    pub static ref FLOAT10_TEN: Float10 = Float10::try_from("10").unwrap();
}

// Utility functions
/// Convert a Float10 to [`dashu::integer::IBig`].
pub fn fbig2ibig(f: Float10) -> IBig {
    let rad = IBig::from(10);
    let sig = f.repr().significand();
    let exp = f.repr().exponent();

    let val: IBig = if exp >= 0 {
        sig.clone() * rad.pow(exp.unsigned_abs())
    } else {
        sig.clone() / rad.pow(exp.unsigned_abs())
    };

    val
}

/// Convert a Float10 to [`pallas::Base`].
/// Note: negative values in pallas field don't wrap,
/// and can't be converted back to original value.
pub fn fbig2base(f: Float10) -> pallas::Base {
    let val: IBig = fbig2ibig(f);
    let (sign, word) = val.as_sign_words();
    let mut words: [u64; 4] = [0, 0, 0, 0];
    words[..word.len()].copy_from_slice(word);
    match sign {
        Sign::Positive => pallas::Base::from_raw(words),
        Sign::Negative => pallas::Base::from_raw(words).neg(),
    }
}

/// Convert a [`pallas::Base`] to [`dashu::integer::IBig`].
/// Note: only zero and positive numbers conversion is supported.
/// Used for testing purposes on non-negative values at the moment.
pub fn base2ibig(base: pallas::Base) -> IBig {
    let byts: [u8; 32] = base.to_repr();
    let words: [u64; 4] = [
        u64::from_le_bytes(byts[0..8].try_into().expect("")),
        u64::from_le_bytes(byts[8..16].try_into().expect("")),
        u64::from_le_bytes(byts[16..24].try_into().expect("")),
        u64::from_le_bytes(byts[24..32].try_into().expect("")),
    ];
    let uparts = UBig::from_words(&words);
    IBig::from_parts(Sign::Positive, uparts)
}

#[cfg(test)]
mod tests {
    use darkfi_sdk::pasta::pallas;
    use dashu::integer::IBig;

    use super::{base2ibig, fbig2base, fbig2ibig, Float10};

    #[test]
    fn dashu_fbig2ibig() {
        let f = Float10::try_from("234234223.000").unwrap();
        let i: IBig = fbig2ibig(f);
        let sig = IBig::from(234234223);
        assert_eq!(i, sig);
    }

    #[test]
    fn dashu_test_base2ibig() {
        let fbig: Float10 = Float10::try_from(
            "289480223093290488558927462521719769633630564819415607159546767643499676303",
        )
        .unwrap();
        let ibig = fbig2ibig(fbig.clone());
        let res_base: pallas::Base = fbig2base(fbig.clone());
        let res_ibig: IBig = base2ibig(res_base);
        assert_eq!(res_ibig, ibig);
    }

    #[test]
    fn dashu_test2_base2ibig() {
        // Verify that field wrapping for negative values won't hold during conversions.
        let fbig: Float10 = Float10::try_from(
            "-20065240046497827215558476051577517633529246907153511707181011345840062564.87",
        )
        .unwrap();
        let ibig = fbig2ibig(fbig.clone());
        let res_base: pallas::Base = fbig2base(fbig.clone());
        let res_ibig: IBig = base2ibig(res_base);
        assert_ne!(res_ibig, ibig);
    }
}
