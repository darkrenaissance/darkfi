#!/bin/bash -x
#python scripts/preprocess.py proofs/jubjub.psm > /tmp/jubjub.psm || exit $?
racket lisp/jj.rkt
python scripts/compile.py --supervisor jj.psm --output jubjub.zcd || exit $?
cargo run --release --bin jubjub

