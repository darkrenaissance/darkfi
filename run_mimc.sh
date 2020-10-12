#!/bin/bash -x
python scripts/preprocess.py proofs/mimc.psm > /tmp/mimc.psm || exit $?
python scripts/compile.py --supervisor /tmp/mimc.psm --output mimc.zcd || exit $?
cargo run --release --bin mimc

