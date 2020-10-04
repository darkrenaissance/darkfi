#!/bin/bash -x
python scripts/vm.py --rust proofs/vm.pism > src/vm_load.rs
cargo run --release --bin vmtest

