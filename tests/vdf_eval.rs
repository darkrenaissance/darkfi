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

//! Test unit for evaluating VDF speed
// cargo test --release --all-features --test vdf_eval -- --nocapture --include-ignored
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use darkfi_sdk::{crypto::mimc_vdf, num_bigint::BigUint, num_traits::Num};
use prettytable::{format, row, Table};

#[test]
#[ignore]
fn evaluate_vdf() {
    let steps = [
        1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000, 9000, 10000, 15000, 20000, 50000, 100000,
        150000, 200000, 250000, 500000, 1000000, 1250000, 1500000, 1750000, 2000000,
    ];

    let challenge = blake3::hash(b"69420").to_hex();
    let challenge = BigUint::from_str_radix(&challenge, 16).unwrap();

    let mut map: HashMap<u64, (Duration, Duration)> = HashMap::new();

    for n_steps in steps {
        let now = Instant::now();
        print!("E with N={n_steps} ... ");
        let witness = mimc_vdf::eval(&challenge, n_steps);
        let eval_elapsed = now.elapsed();
        println!("{eval_elapsed:?}");

        let now = Instant::now();
        print!("V with N={n_steps} ... ");
        assert!(mimc_vdf::verify(&challenge, n_steps, &witness));
        let verify_elapsed = now.elapsed();
        println!("{verify_elapsed:?}");

        map.insert(n_steps, (eval_elapsed, verify_elapsed));
    }

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(row!["n_steps", "eval time", "verify time"]);
    for n_steps in steps {
        let (eval, verify) = map.get(&n_steps).unwrap();
        table.add_row(row![format!("{n_steps}"), format!("{eval:?}"), format!("{verify:?}")]);
    }

    println!("\n\n{table}");
}
