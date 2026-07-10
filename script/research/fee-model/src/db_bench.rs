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

//! Standalone database microbenchmark for fee constant calibration.
//!
//! Measures sled I/O costs across four scenarios and eight payload sizes
//! to characterize the per-byte cost model. 
//!
//! Usage:
//!     make db_bench
//!     cargo run --release --bin db_bench > db_bench_results.json

use std::{
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
};

#[path = "bench_util.rs"]
mod bench_util;

use serde::{Deserialize, Serialize};
use sled_overlay::sled;
use wasmer::{Imports, Instance, Module, Store, Value};
use wasmer_compiler_singlepass::Singlepass;

const ITERATIONS: usize = 1_000_000;

const PAYLOAD_SIZES: [(&str, usize); 8] = [
    ("32b", 32),
    ("128b", 128),
    ("256b", 256),
    ("512b", 512),
    ("1kib", 1024),
    ("2kib", 2048),
    ("4kib", 4096),
    ("8kib", 8192),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SizeStats {
    p50_ns: u64,
    mean_ns: u64,
    iterations: usize,
    payload_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScenarioStats {
    scenario: String,
    sizes: Vec<(String, SizeStats)>,
    /// Linear regression slope: ns per byte (derived from p50 across all sizes).
    slope_ns_per_byte: f64,
    /// Regression intercept: fixed overhead in ns.
    intercept_ns: f64,
    /// R-squared goodness of fit.
    r_squared: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DbBenchResults {
    wasm_add_p50_ns: u64,
    db_set_new: ScenarioStats,
    db_set_overwrite: ScenarioStats,
    db_get: ScenarioStats,
    db_contains_key: ScenarioStats,
    ratios: Ratios,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Ratios {
    /// db_set_new slope / wasm_add p50
    write_new_per_byte: f64,
    /// db_set_overwrite slope / wasm_add p50
    write_overwrite_per_byte: f64,
    /// db_get slope / wasm_add p50
    read_per_byte: f64,
    /// db_contains_key slope / wasm_add p50
    contains_key_per_byte: f64,
    /// write_new / read
    write_read_ratio: f64,
}

fn bench_key(i: usize) -> [u8; 32] {
    let mut key = [0u8; 32];
    key[..8].copy_from_slice(&(i as u64).to_le_bytes());
    key
}

fn p50(times: &mut Vec<u64>, iters: usize) -> u64 {
    times.sort_unstable();
    times[iters / 2]
}

fn mean(times: &[u64], iters: usize) -> u64 {
    times.iter().map(|&x| x as f64).sum::<f64>() as u64 / iters as u64
}

/// Linear regression of p50 times against payload sizes.
/// Returns (slope_ns_per_byte, intercept_ns, r_squared).
fn linear_regression(stats: &[(usize, u64)]) -> (f64, f64, f64) {
    let n = stats.len() as f64;
    let sum_x: f64 = stats.iter().map(|(bytes, _)| *bytes as f64).sum();
    let sum_y: f64 = stats.iter().map(|(_, p50)| *p50 as f64).sum();
    let sum_xy: f64 = stats.iter().map(|(b, p)| (*b as f64) * (*p as f64)).sum();
    let sum_x2: f64 = stats.iter().map(|(b, _)| (*b as f64).powi(2)).sum();

    let denom = n * sum_x2 - sum_x * sum_x;
    let slope = if denom == 0.0 { 0.0 } else { (n * sum_xy - sum_x * sum_y) / denom };
    let intercept = (sum_y - slope * sum_x) / n;

    let mean_y = sum_y / n;
    let ss_tot: f64 = stats.iter().map(|(_, p)| (*p as f64 - mean_y).powi(2)).sum();
    let ss_res: f64 =
        stats.iter().map(|(b, p)| (*p as f64 - (intercept + slope * *b as f64)).powi(2)).sum();
    let r_squared = if ss_tot == 0.0 { 1.0 } else { 1.0 - ss_res / ss_tot };

    (slope, intercept, r_squared)
}

fn measure_wasm_add() -> u64 {
    let mut store = Store::new(Singlepass::new());
    let module = Module::new(&store, bench_util::WASM_ADD).unwrap();
    let instance = Instance::new(&mut store, &module, &Imports::new()).unwrap();
    let func = instance.exports.get_function("add").unwrap();
    let times = bench_util::collect_times(
        || {
            black_box(func.call(&mut store, &[Value::I32(10), Value::I32(20)]).unwrap());
        },
        ITERATIONS,
    );
    let mut t = times;
    p50(&mut t, ITERATIONS)
}

/// Benchmark inserting unique keys into a growing tree (new-key writes).
fn measure_db_set_new() -> ScenarioStats {
    let config = sled::Config::new().temporary(true).path("fee_db_bench_set_new");
    let db = config.open().unwrap();

    let mut size_stats = vec![];

    for (name, size) in &PAYLOAD_SIZES {
        let tree = db.open_tree(format!("set_new_{name}")).unwrap();
        let value = vec![42u8; *size];
        let mut idx = 0usize;

        let times = bench_util::collect_times(
            || {
                let key = bench_key(idx);
                idx += 1;
                tree.insert(key.as_slice(), value.as_slice()).unwrap();
            },
            ITERATIONS,
        );
        let mut t = times;
        let p = p50(&mut t, ITERATIONS);
        let m = mean(&t, ITERATIONS);
        size_stats.push((
            name.to_string(),
            SizeStats { p50_ns: p, mean_ns: m, iterations: ITERATIONS, payload_bytes: *size },
        ));
        db.drop_tree(format!("set_new_{name}")).unwrap();
    }

    let regression_data: Vec<(usize, u64)> =
        size_stats.iter().map(|(_, s)| (s.payload_bytes, s.p50_ns)).collect();
    let (slope, intercept, r2) = linear_regression(&regression_data);

    ScenarioStats {
        scenario: "db_set_new".to_string(),
        sizes: size_stats,
        slope_ns_per_byte: slope,
        intercept_ns: intercept,
        r_squared: r2,
    }
}

/// Benchmark overwriting existing keys (no tree growth).
fn measure_db_set_overwrite() -> ScenarioStats {
    let config = sled::Config::new().temporary(true).path("fee_db_bench_set_overwrite");
    let db = config.open().unwrap();

    let mut size_stats = vec![];

    for (name, size) in &PAYLOAD_SIZES {
        let tree = db.open_tree(format!("set_ow_{name}")).unwrap();
        let value = vec![42u8; *size];

        // Pre-populate all keys
        for i in 0..ITERATIONS {
            let key = bench_key(i);
            tree.insert(key.as_slice(), value.as_slice()).unwrap();
        }
        tree.flush().unwrap();

        // Timed loop: overwrite existing keys in round-robin
        let idx = AtomicUsize::new(0);
        let times = bench_util::collect_times(
            || {
                let current = idx.fetch_add(1, Ordering::Relaxed) % ITERATIONS;
                let key = bench_key(current);
                tree.insert(key.as_slice(), value.as_slice()).unwrap();
            },
            ITERATIONS,
        );
        let mut t = times;
        let p = p50(&mut t, ITERATIONS);
        let m = mean(&t, ITERATIONS);
        size_stats.push((
            name.to_string(),
            SizeStats { p50_ns: p, mean_ns: m, iterations: ITERATIONS, payload_bytes: *size },
        ));
        db.drop_tree(format!("set_ow_{name}")).unwrap();
    }

    let regression_data: Vec<(usize, u64)> =
        size_stats.iter().map(|(_, s)| (s.payload_bytes, s.p50_ns)).collect();
    let (slope, intercept, r2) = linear_regression(&regression_data);

    ScenarioStats {
        scenario: "db_set_overwrite".to_string(),
        sizes: size_stats,
        slope_ns_per_byte: slope,
        intercept_ns: intercept,
        r_squared: r2,
    }
}

/// Benchmark reading existing keys by payload size.
fn measure_db_get() -> ScenarioStats {
    let config = sled::Config::new().temporary(true).path("fee_db_bench_get");
    let db = config.open().unwrap();

    let mut size_stats = vec![];

    for (name, size) in &PAYLOAD_SIZES {
        let tree = db.open_tree(format!("get_{name}")).unwrap();
        let value = vec![42u8; *size];

        // Pre-populate
        for i in 0..ITERATIONS {
            let key = bench_key(i);
            tree.insert(key.as_slice(), value.as_slice()).unwrap();
        }
        tree.flush().unwrap();

        // Timed loop: read in round-robin
        let idx = AtomicUsize::new(0);
        let times = bench_util::collect_times(
            || {
                let current = idx.fetch_add(1, Ordering::Relaxed) % ITERATIONS;
                let key = bench_key(current);
                black_box(tree.get(key.as_slice()).unwrap());
            },
            ITERATIONS,
        );
        let mut t = times;
        let p = p50(&mut t, ITERATIONS);
        let m = mean(&t, ITERATIONS);
        size_stats.push((
            name.to_string(),
            SizeStats { p50_ns: p, mean_ns: m, iterations: ITERATIONS, payload_bytes: *size },
        ));
        db.drop_tree(format!("get_{name}")).unwrap();
    }

    let regression_data: Vec<(usize, u64)> =
        size_stats.iter().map(|(_, s)| (s.payload_bytes, s.p50_ns)).collect();
    let (slope, intercept, r2) = linear_regression(&regression_data);

    ScenarioStats {
        scenario: "db_get".to_string(),
        sizes: size_stats,
        slope_ns_per_byte: slope,
        intercept_ns: intercept,
        r_squared: r2,
    }
}

/// Benchmark contains_key on existing keys by payload size.
/// Note: contains_key cost should not depend on value size, but we measure
/// across payload sizes anyway to confirm this and detect any tree-depth effects.
fn measure_db_contains_key() -> ScenarioStats {
    let config = sled::Config::new().temporary(true).path("fee_db_bench_contains");
    let db = config.open().unwrap();

    let mut size_stats = vec![];

    for (name, size) in &PAYLOAD_SIZES {
        let tree = db.open_tree(format!("contains_{name}")).unwrap();
        let value = vec![42u8; *size];

        // Pre-populate
        for i in 0..ITERATIONS {
            let key = bench_key(i);
            tree.insert(key.as_slice(), value.as_slice()).unwrap();
        }
        tree.flush().unwrap();

        // Timed loop: contains_key in round-robin (mix of hits and a few misses)
        let idx = AtomicUsize::new(0);
        let times = bench_util::collect_times(
            || {
                let current = idx.fetch_add(1, Ordering::Relaxed) % (ITERATIONS + 100);
                let key = bench_key(current);
                black_box(tree.contains_key(key.as_slice()).unwrap());
            },
            ITERATIONS,
        );
        let mut t = times;
        let p = p50(&mut t, ITERATIONS);
        let m = mean(&t, ITERATIONS);
        size_stats.push((
            name.to_string(),
            SizeStats { p50_ns: p, mean_ns: m, iterations: ITERATIONS, payload_bytes: *size },
        ));
        db.drop_tree(format!("contains_{name}")).unwrap();
    }

    let regression_data: Vec<(usize, u64)> =
        size_stats.iter().map(|(_, s)| (s.payload_bytes, s.p50_ns)).collect();
    let (slope, intercept, r2) = linear_regression(&regression_data);

    ScenarioStats {
        scenario: "db_contains_key".to_string(),
        sizes: size_stats,
        slope_ns_per_byte: slope,
        intercept_ns: intercept,
        r_squared: r2,
    }
}

fn main() {
    let wasm_add_p50 = measure_wasm_add();

    let set_new = measure_db_set_new();
    let set_overwrite = measure_db_set_overwrite();
    let get = measure_db_get();
    let contains = measure_db_contains_key();

    let baseline = wasm_add_p50 as f64;
    let ratios = Ratios {
        write_new_per_byte: set_new.slope_ns_per_byte / baseline,
        write_overwrite_per_byte: set_overwrite.slope_ns_per_byte / baseline,
        read_per_byte: get.slope_ns_per_byte / baseline,
        contains_key_per_byte: contains.slope_ns_per_byte / baseline,
        write_read_ratio: if get.slope_ns_per_byte > 0.0 {
            set_new.slope_ns_per_byte / get.slope_ns_per_byte
        } else {
            0.0
        },
    };

    let results = DbBenchResults {
        wasm_add_p50_ns: wasm_add_p50,
        db_set_new: set_new,
        db_set_overwrite: set_overwrite,
        db_get: get,
        db_contains_key: contains,
        ratios,
    };

    println!("{}", serde_json::to_string_pretty(&results).unwrap());
}
