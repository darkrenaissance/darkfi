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

use criterion::{criterion_group, criterion_main, Criterion};
use equix_pow::{Challenge, EquiXBuilder, EquiXPow, Solution, SolverMemory, NONCE_LEN};
use rand::{seq::SliceRandom, Rng};
use std::hint::black_box;

fn new_challenge() -> Challenge {
    let mut rng = rand::thread_rng();
    let random: Vec<u8> = (0..32 + NONCE_LEN).map(|_| rng.gen()).collect();
    Challenge(random)
}

fn new_equix() -> EquiXPow {
    EquiXPow {
        effort: 1000,
        challenge: new_challenge(),
        equix: EquiXBuilder::default(),
        mem: SolverMemory::default(),
    }
}

fn benchmark_equix_pow(c: &mut Criterion) {
    let mut solutions: Vec<(Challenge, Solution)> = Vec::new();

    let mut equix_pow = new_equix();
    c.bench_function(&format!("EquiXPow::run effort={}", equix_pow.effort), |b| {
        b.iter(|| {
            equix_pow.challenge = new_challenge();
            let solution = black_box(equix_pow.run().unwrap());
            solutions.push((equix_pow.challenge.clone(), solution));
        });
    });

    let equix_pow = new_equix();
    c.bench_function(&format!("EquiXPow::verify effort={}", equix_pow.effort), |b| {
        b.iter(|| {
            let (challenge, solution) =
                black_box(solutions.choose(&mut rand::thread_rng()).unwrap());
            if let Err(e) = equix_pow.verify(challenge, solution) {
                eprintln!("Verification failed: {:?}", e);
            }
        });
    });
}

criterion_group!(benches, benchmark_equix_pow);
criterion_main!(benches);
