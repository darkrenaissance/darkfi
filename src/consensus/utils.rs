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

use super::Float10;
use dashu::integer::{IBig, Sign};
use log::{debug};
use pasta_curves::pallas;
//use pasta_curves::{group::ff::PrimeField};
//use dashu::integer::{UBig};

pub fn fbig2ibig(f: Float10) -> IBig {
    let rad = IBig::try_from(10).unwrap();
    let sig = f.repr().significand();
    let exp = f.repr().exponent();
    let val: IBig = if exp >= 0 { sig.clone() * rad.pow(exp as usize) } else { sig.clone() };
    debug!("fbig2ibig (f): {}", f);
    debug!("fbig2ibig (i): {}", val);
    val
}
/*
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
    //TODO both y, and t are positive, but workout the sign for general use
    let ibig = IBig::from_parts(Sign::Positive, uparts);
    ibig
}
*/
pub fn fbig2base(f: Float10) -> pallas::Base {
    debug!("fbig -> base (f): {}", f);
    let val: IBig = fbig2ibig(f);
    let (sign, word) = val.as_sign_words();
    let mut words: [u64; 4] = [0, 0, 0, 0];
    words[..word.len()].copy_from_slice(word);
    match sign {
        Sign::Positive => pallas::Base::from_raw(words),
        Sign::Negative => pallas::Base::from_raw(words).neg(),
    }
}

#[cfg(test)]
mod tests {
    use dashu::integer::IBig;

    use crate::consensus::{constants::RADIX_BITS, types::Float10, utils::fbig2ibig};

    #[test]
    fn dashu_fbig2ibig() {
        let f =
            Float10::from_str_native("234234223.000").unwrap().with_precision(RADIX_BITS).value();
        let i: IBig = fbig2ibig(f);
        let sig = IBig::from(234234223);
        assert_eq!(i, sig);
    }

    /*
    #[test]
    fn dashu_test_base2ibig() {
        //
        let fbig: Float10 = Float10::from_str_native(
            "28948022309329048855892746252171976963363056481941560715954676764349967630337",
        )
        .unwrap()
        .with_precision(RADIX_BITS)
        .value();
        let ibig = fbig2ibig(fbig.clone());
        let res_base: pallas::Base = fbig2base(fbig.clone());
        let res_ibig: IBig = base2ibig(res_base);
        assert_eq!(res_ibig, ibig);
    }
    */
}
