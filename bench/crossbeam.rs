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

use criterion::{black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use crossbeam_skiplist::SkipMap;
use easy_parallel::Parallel;
use rand::{rngs::OsRng, Rng};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

fn crossbeam(c: &mut Criterion) {
    let mut group = c.benchmark_group("crossbeam_vs_mutex-hashmap_writes");

    for k in 1..10 {
        let stopped = Arc::new(AtomicBool::new(false));
        let map = Arc::new(SkipMap::new());

        let stopped2 = stopped.clone();
        let map2 = map.clone();

        // Start n threads all doing continuous inserts until we tell them to stop
        let parallel_inserts = std::thread::spawn(move || {
            Parallel::new()
                .each(0..k, |_| {
                    let stopped = stopped2.clone();
                    let map = map2.clone();

                    while !stopped.load(Ordering::Relaxed) {
                        let key: usize = OsRng.gen();
                        let val: usize = OsRng.gen();
                        map.insert(key, val);
                    }
                })
                .run();
        });

        group.bench_with_input(BenchmarkId::new("crossbeam", k), &k, |b, &_| {
            b.iter_batched(
                || {
                    let key: usize = OsRng.gen();
                    let val: usize = OsRng.gen();
                    (key, val)
                },
                |(key, val)| {
                    map.insert(key, val);
                },
                BatchSize::SmallInput,
            )
        });

        stopped.store(true, Ordering::Relaxed);
        parallel_inserts.join().unwrap();
    }

    // Now try normal Mutex hashmap
    // This is not an async Mutex, but async Mutexes are always slower than sync ones anyway
    // since they just implement an async interface on top of sync Mutexes.

    for k in 1..10 {
        let stopped = Arc::new(AtomicBool::new(false));
        let map = Arc::new(Mutex::new(HashMap::new()));

        let stopped2 = stopped.clone();
        let map2 = map.clone();

        // Start n threads all doing continuous inserts until we tell them to stop
        let parallel_inserts = std::thread::spawn(move || {
            Parallel::new()
                .each(0..k, |_| {
                    let stopped = stopped2.clone();
                    let map = map2.clone();

                    while !stopped.load(Ordering::Relaxed) {
                        let key: usize = OsRng.gen();
                        let val: usize = OsRng.gen();
                        map.lock().unwrap().insert(key, val);
                    }
                })
                .run();
        });

        group.bench_with_input(BenchmarkId::new("mutex_hashmap", k), &k, |b, &_| {
            b.iter_batched(
                || {
                    let key: usize = OsRng.gen();
                    let val: usize = OsRng.gen();
                    (key, val)
                },
                |(key, val)| {
                    map.lock().unwrap().insert(key, val);
                },
                BatchSize::SmallInput,
            )
        });

        stopped.store(true, Ordering::Relaxed);
        parallel_inserts.join().unwrap();
    }

    group.finish();

    let mut group = c.benchmark_group("crossbeam_vs_mutex-hashmap_reads");

    for k in 1..10 {
        let stopped = Arc::new(AtomicBool::new(false));
        let map = Arc::new(SkipMap::new());

        let stopped2 = stopped.clone();
        let map2 = map.clone();

        // Start n threads all doing continuous inserts until we tell them to stop
        let parallel_inserts = std::thread::spawn(move || {
            Parallel::new()
                .each(0..k, |_| {
                    let stopped = stopped2.clone();
                    let map = map2.clone();

                    while !stopped.load(Ordering::Relaxed) {
                        let key: usize = OsRng.gen();
                        let val: usize = OsRng.gen();
                        map.insert(key, val);
                    }
                })
                .run();
        });

        group.bench_with_input(BenchmarkId::new("crossbeam", k), &k, |b, &_| {
            b.iter(|| {
                for entry in map.iter().take(100) {
                    let key = entry.key();
                    let val = entry.value();
                    black_box((key, val));
                }
            })
        });

        stopped.store(true, Ordering::Relaxed);
        parallel_inserts.join().unwrap();
    }

    // Now try normal Mutex hashmap
    // This is not an async Mutex, but async Mutexes are always slower than sync ones anyway
    // since they just implement an async interface on top of sync Mutexes.

    for k in 1..10 {
        let stopped = Arc::new(AtomicBool::new(false));
        let map = Arc::new(Mutex::new(HashMap::new()));

        let stopped2 = stopped.clone();
        let map2 = map.clone();

        // Start n threads all doing continuous inserts until we tell them to stop
        let parallel_inserts = std::thread::spawn(move || {
            Parallel::new()
                .each(0..k, |_| {
                    let stopped = stopped2.clone();
                    let map = map2.clone();

                    while !stopped.load(Ordering::Relaxed) {
                        let key: usize = OsRng.gen();
                        let val: usize = OsRng.gen();
                        map.lock().unwrap().insert(key, val);
                    }
                })
                .run();
        });

        group.bench_with_input(BenchmarkId::new("mutex_hashmap", k), &k, |b, &_| {
            b.iter(|| {
                for (key, val) in map.lock().unwrap().iter().take(100) {
                    // Do nothing
                    black_box((key, val));
                }
            })
        });

        stopped.store(true, Ordering::Relaxed);
        parallel_inserts.join().unwrap();
    }
}

criterion_group!(bench, crossbeam);
criterion_main!(bench);
