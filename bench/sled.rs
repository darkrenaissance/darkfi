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

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use darkfi_sdk::crypto::pasta_prelude::*;
use halo2_proofs::pasta::Fp;
use rand::rngs::OsRng;

fn sled(c: &mut Criterion) {
    let db = sled::open("/tmp/db").unwrap();
    let tree = db.open_tree(b"hello").unwrap();

    let mut group = c.benchmark_group("inserts");
    for i in 0..10 {
        println!("i={}", i);
        // Insert 1 million keys
        for j in 0..1_000_000 {
            if j % 100000 == 0 {
                println!("  inserted {} values...", j);
            }
            let a = Fp::random(&mut OsRng).to_repr();
            tree.insert(&a, &[]).unwrap();
        }
        let x = Fp::random(&mut OsRng).to_repr();
        group.bench_with_input(BenchmarkId::from_parameter(i), &i, |b, &_| {
            b.iter_batched(|| tree.remove(&x), |_| tree.insert(&x, &[]), BatchSize::SmallInput)
        });
    }
    tree.clear().unwrap();
    let _ = db.drop_tree(b"hello");
}

criterion_group!(bench, sled);
criterion_main!(bench);
