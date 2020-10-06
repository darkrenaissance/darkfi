#!/bin/bash -x
python scripts/preprocess.py proofs/bits.psm > /tmp/bits.psm
python scripts/vm.py --rust /tmp/bits.psm > src/bits_contract.rs
cargo run --release --bin bits

