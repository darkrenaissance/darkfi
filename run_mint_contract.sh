#!/bin/bash -x
python scripts/preprocess.py proofs/mint.pism > /tmp/mint.pism
python scripts/pism.py /tmp/mint.pism proofs/mint.aux | rustfmt > src/mint_contract.rs
cargo run --release --bin mint
