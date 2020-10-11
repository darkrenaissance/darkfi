#!/bin/bash -x
python scripts/preprocess.py proofs/jubjub.psm > /tmp/jubjub.psm || exit $?
python scripts/compile.py --supervisor /tmp/jubjub.psm --output jubjub.zcd || exit $?
cargo run --release --bin jubjub

