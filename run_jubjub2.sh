#!/bin/bash -x
racket lisp/jj.rkt || exit $?
python scripts/compile.py --supervisor jj.psm --output jubjub.zcd || exit $?
cargo run --release --bin jubjub
cargo run --bin zkvm -- init jubjub.zcd jubjub.zts
cargo run --bin zkvm -- prove jubjub.zcd jubjub.zts proofs/jubjub.params jubjub.prf
cargo run --bin zkvm -- verify jubjub.zcd jubjub.zts jubjub.prf
cargo run --bin zkvm -- show jubjub.prf
