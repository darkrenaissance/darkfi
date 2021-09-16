#!/bin/bash -x
python scripts/pism.py proofs/simple.pism | rustfmt > src/simple_circuit.rs
cargo run --release --bin simple

