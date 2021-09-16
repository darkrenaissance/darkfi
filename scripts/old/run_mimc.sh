#!/bin/bash -x
python scripts/preprocess.py proofs/mimc.psm > /tmp/mimc.psm
python scripts/vm.py --rust /tmp/mimc.psm > src/zkmimc_contract.rs
cargo run --release --bin zkmimc

