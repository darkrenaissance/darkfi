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

use darkfi::consensus::lead_coin::LeadCoin;
use darkfi_sdk::{crypto::pasta_prelude::PrimeField, pasta::pallas};

/// Generate a string represenation of a pallas::Base constant
fn to_constant(name: &str, pallas: pallas::Base) -> String {
    let repr = pallas.to_repr();
    let mut res = [0; 4];
    res[0] = u64::from_le_bytes(repr[0..8].try_into().unwrap());
    res[1] = u64::from_le_bytes(repr[8..16].try_into().unwrap());
    res[2] = u64::from_le_bytes(repr[16..24].try_into().unwrap());
    res[3] = u64::from_le_bytes(repr[24..32].try_into().unwrap());

    format!("    const {name}: pallas::Base = pallas::Base::from_raw({res:?});")
}

/// Generate constants for corresponding pallas::Base.
fn main() {
    let headstart = to_constant("HEADSTART", LeadCoin::headstart());
    println!("Constants:");
    println!("{headstart}");
}

#[cfg(test)]
mod tests {
    use darkfi::consensus::lead_coin::LeadCoin;
    use darkfi_sdk::pasta::pallas;

    const HEADSTART: pallas::Base = pallas::Base::from_raw([
        11731824086999220879,
        11830614503713258191,
        737869762948382064,
        46116860184273879,
    ]);

    #[test]
    fn test_headstart() {
        let headstart = LeadCoin::headstart();
        assert_eq!(headstart, HEADSTART);
    }
}
