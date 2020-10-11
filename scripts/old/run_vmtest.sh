#!/bin/bash -x
python scripts/preprocess.py proofs/jubjub.pism > /tmp/jubjub.pism
python scripts/vm.py --rust /tmp/jubjub.pism > src/vm_load.rs
cargo run --release --bin vmtest

