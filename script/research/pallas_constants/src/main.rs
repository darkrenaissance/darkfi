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

use std::{
    io,
    io::Write,
    process::{Command, Stdio},
};

use anyhow::Result;
use darkfi::{
    consensus::lead_coin::LeadCoin,
    validator::consensus::float_10::{fbig2base, Float10},
};
use darkfi_sdk::{crypto::pasta_prelude::PrimeField, pasta::pallas};

/// Generate a string represenation of a `pallas::Base` constant
fn to_constant(name: &str, x: pallas::Base, public: bool) -> String {
    let repr = x.to_repr();
    let mut res = [0_u64; 4];

    res[0] = u64::from_le_bytes(repr[0..8].try_into().unwrap());
    res[1] = u64::from_le_bytes(repr[8..16].try_into().unwrap());
    res[2] = u64::from_le_bytes(repr[16..24].try_into().unwrap());
    res[3] = u64::from_le_bytes(repr[24..32].try_into().unwrap());

    let p = if public { "pub" } else { "" };

    format!("{p} const {name}: pallas::Base = pallas::Base::from_raw({res:?});\n")
}

/// Generate constants for corresponding `pallas::Base`
fn main() -> Result<()> {
    let mut source = String::new();
    source.push_str(&to_constant("REWARD_PALLAS", pallas::Base::from(100_000_000), true));
    source.push_str(&to_constant("SERIAL_PREFIX", pallas::Base::from(2), true));
    source.push_str(&to_constant("SEED_PREFIX", pallas::Base::from(3), true));
    source.push_str(&to_constant("SECRET_KEY_PREFIX", pallas::Base::from(4), true));
    source.push_str(&to_constant("MU_Y_PREFIX", pallas::Base::from(22), true));
    source.push_str(&to_constant("MU_RHO_PREFIX", pallas::Base::from(5), true));
    source.push_str(&to_constant("HEADSTART", LeadCoin::headstart(), true));

    // P
    const P: &str = "28948022309329048855892746252171976963363056481941560715954676764349967630337";
    let p = Float10::try_from(P).unwrap();
    let one = Float10::try_from("1").unwrap();
    source.push_str(&to_constant("P", fbig2base(p.clone()), false));
    source.push_str(&to_constant("P_MINUS_1", fbig2base(p - one), false));

    let mut cmd = Command::new("rustfmt");
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
    cmd.args(["--edition=2021"]);

    let mut child = cmd.spawn()?;
    let mut child_stdin = child.stdin.take().unwrap();
    let mut child_stdout = child.stdout.take().unwrap();

    let stdin_handle = std::thread::spawn(move || {
        let _ = child_stdin.write_all(source.as_bytes());
        source
    });

    let mut output = vec![];
    io::copy(&mut child_stdout, &mut output)?;

    let _ = stdin_handle.join().unwrap();
    let output = String::from_utf8(output)?;
    print!("{}", output);

    Ok(())
}

#[cfg(test)]
mod tests {
    use darkfi::consensus::lead_coin::LeadCoin;
    use darkfi_sdk::pasta::pallas;

    #[test]
    fn consistency() {
        let zero = pallas::Base::zero();
        let zero_arr = [0, 0, 0, 0];

        let reward = pallas::Base::one();
        let reward_arr = [1, 0, 0, 0];

        let serial_prefix = pallas::Base::from(2);
        let serial_prefix_arr = [2, 0, 0, 0];

        let seed_prefix = pallas::Base::from(3);
        let seed_prefix_arr = [3, 0, 0, 0];

        let mu_y_prefix = pallas::Base::from(22);
        let mu_y_prefix_arr = [22, 0, 0, 0];

        let mu_rho_prefix = pallas::Base::from(5);
        let mu_rho_prefix_arr = [5, 0, 0, 0];

        let headstart = LeadCoin::headstart();
        let headstart_arr =
            [11731824086999220879, 11830614503713258191, 737869762948382064, 46116860184273879];

        assert_eq!(zero, pallas::Base::from_raw(zero_arr));
        assert_eq!(reward, pallas::Base::from_raw(reward_arr));
        assert_eq!(serial_prefix, pallas::Base::from_raw(serial_prefix_arr));
        assert_eq!(seed_prefix, pallas::Base::from_raw(seed_prefix_arr));
        assert_eq!(mu_y_prefix, pallas::Base::from_raw(mu_y_prefix_arr));
        assert_eq!(mu_rho_prefix, pallas::Base::from_raw(mu_rho_prefix_arr));
        assert_eq!(headstart, pallas::Base::from_raw(headstart_arr));
    }
}
