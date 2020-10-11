#!/bin/bash -x
python scripts/preprocess.py proofs/mint2.psm > /tmp/mint2.psm || exit $?
python scripts/vm.py --rust /tmp/mint2.psm > src/mint2_contract.rs || exit $?
cargo run --release --bin mint2

