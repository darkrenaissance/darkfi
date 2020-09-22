#!/bin/bash -x
python scripts/pism.py proofs/mint.pism | rustfmt > src/mint_contract.rs
cargo run --release --bin mint
