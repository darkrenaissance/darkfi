#!/bin/bash -x
python3 scripts/preprocess.py proofs/mimc.psm > /tmp/mimc.psm || exit $?
python3 scripts/compile.py --supervisor /tmp/mimc.psm --output mimc.zcd || exit $?
cargo run --release --bin mimc

