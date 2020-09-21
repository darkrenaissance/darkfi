#!/bin/bash -x
python scripts/pism.py proofs/simple.pism > src/simple_circuit.rs
cargo fmt
cargo run --release --bin simple

