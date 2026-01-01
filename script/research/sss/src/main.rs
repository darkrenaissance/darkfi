/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! A simple implementation of Shamir Secret Sharing using the pallas base field

use pasta_curves::{group::ff::Field, pallas};
use rand::{prelude::SliceRandom, rngs::OsRng};

#[derive(Copy, Clone, Debug)]
struct ShamirPoint {
    pub x: pallas::Base,
    pub y: pallas::Base,
}

fn sss_share(secret: pallas::Base, n_shares: usize, threshold: usize) -> Vec<ShamirPoint> {
    assert!(threshold > 2 && n_shares > threshold);

    let mut coeffs = vec![secret];
    for _ in 0..threshold - 1 {
        coeffs.push(pallas::Base::random(&mut OsRng));
    }

    let mut shares = Vec::with_capacity(n_shares);
    for x in 1..n_shares + 1 {
        let x = pallas::Base::from(x as u64);
        let mut y = pallas::Base::zero();
        for coeff in coeffs.iter().rev() {
            y *= x;
            y += coeff;
        }

        shares.push(ShamirPoint { x, y });
    }

    shares
}

fn sss_recover(shares: &[ShamirPoint]) -> pallas::Base {
    assert!(shares.len() > 1);

    let mut secret = pallas::Base::zero();

    for (j, share_j) in shares.iter().enumerate() {
        let mut prod = pallas::Base::one();

        for (i, share_i) in shares.iter().enumerate() {
            if i != j {
                prod *= share_i.x * (share_i.x - share_j.x).invert().unwrap();
            }
        }

        prod *= share_j.y;
        secret += prod;
    }

    secret
}

fn main() {
    let random_secret = pallas::Base::random(&mut OsRng);
    let shares = sss_share(random_secret, 700, 300);
    let sample: Vec<ShamirPoint> = shares.choose_multiple(&mut OsRng, 300).copied().collect();
    let recovered_secret = sss_recover(&sample);
    assert_eq!(random_secret, recovered_secret);
}
