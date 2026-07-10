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

use std::{fs, hint::black_box, io::Cursor, path::PathBuf};

#[path = "bench_util.rs"]
mod bench_util;

use bridgetree::Hashable;
use darkfi::{
    zk::{empty_witnesses, Proof, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::crypto::{
    keypair::{Keypair, PublicKey},
    merkle_node::MerkleNode,
    schnorr::{SchnorrPublic, SchnorrSecret},
    util::poseidon_hash,
};
use darkfi_serial as serial;
use pasta_curves::{group::ff::Field, pallas};
use serde::{Deserialize, Serialize};
use wasmer::{Imports, Instance, Module, Store, Value};
use wasmer_compiler_singlepass::Singlepass;

/// Number of iterations for each benchmark measurement
const ITERATIONS: usize = 1_000_000;

/// Number of iterations for ZK (expensive operations)
const ZK_ITERATIONS: usize = 1000;

/// Statistics for benchmark measurements
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchmarkStats {
    mean_ns: u64,    // Mean (average)
    p50_ns: u64,     // Median
    p90_ns: u64,     // 90th percentile
    p99_ns: u64,     // 99th percentile
    max_ns: u64,     // Maximum observed
    std_dev_ns: u64, // Standard deviation
    iterations: usize,
}

/// Statistics for ZK circuit verification at different k values
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ZkVerifyStats {
    k_11: BenchmarkStats, // 2^11 = 2,048 rows
    k_14: BenchmarkStats, // 2^14 = 16,384 rows
}

/// Statistics for ZK circuit compilation (VerifyingKey building) at different k values
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ZkCompileStats {
    k_11: BenchmarkStats, // 2^11 = 2,048 rows
    k_14: BenchmarkStats, // 2^14 = 16,384 rows
}

/// Per-circuit benchmark results with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CircuitBenchmark {
    name: String,
    k: u32,
    /// Per-row compilation stats (median)
    compile_p50_ns_per_row: u64,
    /// VerifyingKey size in bytes
    vk_size_bytes: usize,
    /// Circuit metadata extracted from zk.bin
    opcodes_count: usize,
    witnesses_count: usize,
    literals_count: usize,
}

/// Calculate percentiles from times
fn percentiles(mut times: Vec<u64>, iters: usize) -> BenchmarkStats {
    times.sort_unstable();
    let p50 = times[iters / 2];
    let p90 = times[iters * 90 / 100];
    let p99 = times[iters * 99 / 100];
    let max = times[iters - 1];

    // Calculate mean and standard deviation
    let mean = times.iter().map(|&x| x as f64).sum::<f64>() / iters as f64;
    let variance = times
        .iter()
        .map(|&x| {
            let diff = x as f64 - mean;
            diff * diff
        })
        .sum::<f64>()
        / iters as f64;
    let std_dev = variance.sqrt() as u64;

    BenchmarkStats {
        mean_ns: mean as u64,
        p50_ns: p50,
        p90_ns: p90,
        p99_ns: p99,
        max_ns: max,
        std_dev_ns: std_dev,
        iterations: iters,
    }
}

/// Measure execution time for an operation with statistics.
fn measure<F: FnMut()>(op: F, iters: usize) -> BenchmarkStats {
    percentiles(bench_util::collect_times(op, iters), iters)
}

/// Measure execution time for a ZK circuit operation, returning per-row statistics.
/// Includes a minimal warmup phase to handle cold-start overhead without significant runtime cost.
fn measure_zk<F: FnMut()>(mut op: F, iters: usize, k: u32) -> BenchmarkStats {
    // Minimal warmup (5 iterations) to handle cold-start overhead for expensive ZK ops.
    // This adds negligible overhead (~0.5% for 1000 iterations) while stabilizing measurements.
    for _ in 0..5 {
        let _ = op();
    }

    let rows = (1usize << k) as u64;
    let times = bench_util::collect_times(op, iters);
    let mut stats = percentiles(times, iters);
    stats.mean_ns /= rows;
    stats.p50_ns /= rows;
    stats.p90_ns /= rows;
    stats.p99_ns /= rows;
    stats.max_ns /= rows;
    stats.std_dev_ns /= rows;
    stats
}

/// Collect all measurements for JSON output
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Measurements {
    wasm_add: BenchmarkStats,
    poseidon_hash: BenchmarkStats,
    sinsemilla_hash: BenchmarkStats,
    pallas_signature_verify: BenchmarkStats,
    zk_verify: ZkVerifyStats,
    zk_compile: ZkCompileStats,
    /// Per-circuit benchmarks with VK sizes
    #[serde(flatten)]
    circuits: std::collections::HashMap<String, CircuitBenchmark>,
}

/// Load a ZK circuit from a .zk.bin file
fn load_zk_circuit(name: &str) -> (ZkCircuit, u32) {
    let zk_bin =
        std::fs::read(&format!("../zkvm-metering/generator/src/opcodes/proof/{}.zk.bin", name))
            .unwrap();
    let zkbin = ZkBinary::decode(&zk_bin, false).unwrap();
    let verifier_witnesses = empty_witnesses(&zkbin).unwrap();
    let circuit = ZkCircuit::new(verifier_witnesses, &zkbin);
    (circuit, zkbin.k)
}

/// Benchmark ZK circuit verification for a given circuit file.
fn measure_zk_verify(name: &str) -> BenchmarkStats {
    let proof_bin =
        std::fs::read(&format!("../zkvm-metering/generator/src/opcodes/proof/{}.proof.bin", name))
            .unwrap();
    let vk_bin =
        std::fs::read(&format!("../zkvm-metering/generator/src/opcodes/proof/{}.vks.bin", name))
            .unwrap();
    let pi_bin =
        std::fs::read(&format!("../zkvm-metering/generator/src/opcodes/proof/{}.pi.bin", name))
            .unwrap();

    let (circuit, k) = load_zk_circuit(name);

    let proof: Proof = serial::deserialize(&proof_bin).unwrap();
    let mut vk_buf = Cursor::new(vk_bin);
    let vk = VerifyingKey::read::<Cursor<Vec<u8>>, ZkCircuit>(&mut vk_buf, circuit).unwrap();
    let public_inputs: Vec<pallas::Base> = serial::deserialize(&pi_bin).unwrap();

    measure_zk(
        || {
            black_box(proof.verify(&vk, &public_inputs).unwrap());
        },
        ZK_ITERATIONS,
        k,
    )
}

/// Benchmark ZK circuit compilation (VerifyingKey building) for a given circuit file.
fn measure_zk_compile(name: &str) -> BenchmarkStats {
    let (circuit, k) = load_zk_circuit(name);

    measure_zk(
        || {
            black_box(VerifyingKey::build(k, &circuit));
        },
        ZK_ITERATIONS,
        k,
    )
}

/// Find all circuit .zk.bin files in the contract directories
fn find_contract_circuits() -> Vec<(String, PathBuf)> {
    let base_path = PathBuf::from("../../../src/contract");
    let mut circuits = Vec::new();

    for contract in &["money", "dao"] {
        let contract_path = base_path.join(format!("{}/proof", contract));
        if let Ok(entries) = fs::read_dir(&contract_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("zk") {
                    let name = path.file_stem().unwrap().to_string_lossy().to_string();
                    circuits.push((name, path.with_extension("zk.bin")));
                }
            }
        }
    }

    circuits.sort();
    circuits
}

/// Benchmark a single contract circuit and return results
fn benchmark_contract_circuit(name: &str, zk_bin_path: &PathBuf) -> CircuitBenchmark {
    // Read the zk.bin file
    let zk_bin = fs::read(zk_bin_path).unwrap();

    // Decode the circuit
    let zkbin = ZkBinary::decode(&zk_bin, false).unwrap();

    let k = zkbin.k;
    let opcodes_count = zkbin.opcodes.len();
    let witnesses_count = zkbin.witnesses.len();
    let literals_count = zkbin.literals.len();

    // Create empty witnesses and circuit
    let verifier_witnesses = empty_witnesses(&zkbin).unwrap();
    let circuit = ZkCircuit::new(verifier_witnesses, &zkbin);

    // Benchmark compilation with fewer iterations for speed
    let circuit_iters = 100;
    let compile_times = bench_util::collect_times(
        || {
            black_box(VerifyingKey::build(zkbin.k, &circuit));
        },
        circuit_iters,
    );
    let mut compile_stats = percentiles(compile_times, circuit_iters);
    let rows = (1usize << k) as u64;
    compile_stats.p50_ns /= rows;

    // Build the VK to measure its size
    let vk = VerifyingKey::build(zkbin.k, &circuit);
    let mut vk_buf = Vec::new();
    vk.write(&mut vk_buf).unwrap();
    let vk_size_bytes = vk_buf.len();

    CircuitBenchmark {
        name: name.to_string(),
        k,
        compile_p50_ns_per_row: compile_stats.p50_ns,
        vk_size_bytes,
        opcodes_count,
        witnesses_count,
        literals_count,
    }
}

fn main() {
    // WASM opcode benchmarks
    let wasm_add = {
        let mut store = Store::new(Singlepass::new());
        let module = Module::new(&store, bench_util::WASM_ADD).unwrap();
        let instance = Instance::new(&mut store, &module, &Imports::new()).unwrap();
        let func = instance.exports.get_function("add").unwrap();
        measure(
            || {
                black_box(func.call(&mut store, &[Value::I32(10), Value::I32(20)]).unwrap());
            },
            ITERATIONS,
        )
    };

    // Generate inputs outside the timed closures; these benchmarks calibrate
    // the primitive operations, not RNG or allocation overhead.
    let mut rng = rand::thread_rng();
    let poseidon_input = (pallas::Base::random(&mut rng), pallas::Base::random(&mut rng));
    let sinsemilla_input = (
        MerkleNode::from(pallas::Base::random(&mut rng)),
        MerkleNode::from(pallas::Base::random(&mut rng)),
    );

    // Hash operations
    let poseidon_time = measure(
        || {
            let (a, b) = black_box(poseidon_input);
            black_box(poseidon_hash::<2>([a, b]));
        },
        ITERATIONS,
    );

    let sinsemilla_time = measure(
        || {
            let (left, right) = black_box(sinsemilla_input);
            black_box(Hashable::combine(0.into(), &left, &right));
        },
        ITERATIONS,
    );

    // Pallas Schnorr signature verification
    let keypair = Keypair::random(&mut rng);
    let public_key = PublicKey::from_secret(keypair.secret);
    let message = b"DarkFi fee calibration benchmark message";
    let signature = keypair.secret.sign(message);

    let signature_verify_time = measure(
        || {
            black_box(public_key.verify(message, &signature));
        },
        ITERATIONS,
    );

    let measurements = Measurements {
        wasm_add,
        poseidon_hash: poseidon_time,
        sinsemilla_hash: sinsemilla_time,
        pallas_signature_verify: signature_verify_time,
        zk_verify: ZkVerifyStats {
            k_11: measure_zk_verify("poseidon_hash"),
            k_14: measure_zk_verify("sparse_merkle_root"),
        },
        zk_compile: ZkCompileStats {
            k_11: measure_zk_compile("poseidon_hash"),
            k_14: measure_zk_compile("sparse_merkle_root"),
        },
        circuits: {
            let contract_circuits = find_contract_circuits();
            if contract_circuits.is_empty() {
                std::collections::HashMap::new()
            } else {
                let mut results = std::collections::HashMap::new();
                for (name, zk_bin_path) in contract_circuits {
                    let result = benchmark_contract_circuit(&name, &zk_bin_path);
                    results.insert(name, result);
                }
                results
            }
        },
    };

    println!("{}", serde_json::to_string_pretty(&measurements).unwrap());
}
