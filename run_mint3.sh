#!/bin/bash -x
#python scripts/preprocess.py proofs/mint2.psm > /tmp/mint2.psm || exit $?
python scripts/preprocess.py proofs/jubjub.pism > /tmp/mint2.psm || exit $?
python scripts/compile.py --supervisor /tmp/mint2.psm --output mint.zcd || exit $?
cargo run --release --bin mint3

