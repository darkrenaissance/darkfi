#!/bin/bash -x
python scripts/preprocess.py proofs/mimc.psm > /tmp/mimc.psm || exit $?
python scripts/compile.py --supervisor /tmp/mimc.psm --output mimc.zcd || exit $?
cargo run --release --bin zkvm -- init mimc.zcd mimc.zts
cargo run --release --bin zkvm -- prove mimc.zcd mimc.zts proofs/mimc.params mimc.prf
cargo run --release --bin zkvm -- verify mimc.zcd mimc.zts mimc.prf
cargo run --release --bin zkvm -- show mimc.prf
