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

use darkfi_sdk::pasta::pallas;
use dashu::integer::{IBig, Sign};
use log::debug;
use dashu::integer::UBig;
use darkfi_sdk::pasta::group::ff::PrimeField;

use super::Float10;

pub fn fbig2ibig(f: Float10) -> IBig {
    let rad = IBig::try_from(10).unwrap();
    let sig = f.repr().significand();
    let exp = f.repr().exponent();

    let val: IBig = if exp >= 0 {
        sig.clone() * rad.pow(exp as usize)
    } else {
        sig.clone() / rad.pow(exp.abs() as usize)
    };

    val
}

/// note! nagative values in pallas field won't wraps, and won't
/// convert back to same value.
pub fn fbig2base(f: Float10) -> pallas::Base {
    debug!(target: "consensus::utils", "fbig -> base (f): {}", f);
    let val: IBig = fbig2ibig(f);
    let (sign, word) = val.as_sign_words();
    let mut words: [u64; 4] = [0, 0, 0, 0];
    words[..word.len()].copy_from_slice(word);
    match sign {
        Sign::Positive => pallas::Base::from_raw(words),
        Sign::Negative => pallas::Base::from_raw(words).neg(),
    }
}

/// note! only support positive conversion, and zero.
/// used for testing purpose on non-negative values at the moment.
pub fn base2ibig(base: pallas::Base) -> IBig {
    //
    let byts: [u8; 32] = base.to_repr();
    let words: [u64; 4] = [
        u64::from_le_bytes(byts[0..8].try_into().expect("")),
        u64::from_le_bytes(byts[8..16].try_into().expect("")),
        u64::from_le_bytes(byts[16..24].try_into().expect("")),
        u64::from_le_bytes(byts[24..32].try_into().expect("")),
    ];
    let uparts = UBig::from_words(&words);
    let ibig = IBig::from_parts(Sign::Positive, uparts);
    ibig
}
#[cfg(test)]
mod tests {
    use dashu::integer::IBig;

    use darkfi_sdk::pasta::{pallas, group::ff::PrimeField};
    use crate::consensus::{constants::RADIX_BITS, types::Float10, utils::{fbig2ibig, fbig2base, base2ibig}};


    #[test]
    fn dashu_fbig2ibig() {
        let f = Float10::try_from("234234223.000").unwrap();
        let i: IBig = fbig2ibig(f);
        let sig = IBig::from(234234223);
        assert_eq!(i, sig);
    }

    #[test]
    fn dashu_test_base2ibig() {
        //
        let fbig: Float10 = Float10::from_str_native("289480223093290488558927462521719769633630564819415607159546767643499676303").unwrap().with_precision(RADIX_BITS).value();
        let ibig = fbig2ibig(fbig.clone());
        let res_base: pallas::Base = fbig2base(fbig.clone());
        let res_ibig: IBig = base2ibig(res_base);
        assert_eq!(res_ibig, ibig);
    }

    #[test]
    fn dashu_test2_base2ibig() {
        //assert that field wrapping for negative values won't hold during conversions.
        let fbig: Float10 = Float10::from_str_native("-20065240046497827215558476051577517633529246907153511707181011345840062564.87").unwrap().with_precision(RADIX_BITS).value();
        let ibig = fbig2ibig(fbig.clone());
        let res_base: pallas::Base = fbig2base(fbig.clone());
        let res_ibig: IBig = base2ibig(res_base);
        assert_ne!(res_ibig, ibig);
    }
}
