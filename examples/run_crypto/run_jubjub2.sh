#!/bin/bash -x
racket lisp/jj.rkt || exit $?
python scripts/compile.py --supervisor jj.psm --output jubjub.zcd || exit $?
cargo run --release --bin zkvm -- init jubjub.zcd jubjub.zts
cargo run --release --bin zkvm -- prove jubjub.zcd jubjub.zts proofs/jubjub.params jubjub.prf
cargo run --release --bin zkvm -- verify jubjub.zcd jubjub.zts jubjub.prf
cargo run --release --bin zkvm -- show jubjub.prf
