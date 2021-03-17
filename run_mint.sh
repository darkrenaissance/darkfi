#!/bin/bash -x
python3 scripts/preprocess.py proofs/mint2.psm > /tmp/mint2.psm || exit $?
python3 scripts/compile.py --supervisor /tmp/mint2.psm --output mint.zcd || exit $?
cargo run --release --bin mint

