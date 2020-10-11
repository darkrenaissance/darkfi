#!/bin/bash -x
python scripts/preprocess.py proofs/spend.pism > /tmp/spend.pism
python scripts/pism.py /tmp/spend.pism proofs/mint.aux | rustfmt > src/spend_contract.rs
cargo run --release --bin spend
