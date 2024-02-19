Working on native smart contracts
=================================

The native network smart contracts are located in `src/contract/`.
Each of the directories contains a `Makefile` which defines the rules
of building the wasm binary, and target for running tests.

The `Makefile` also contains a `clippy` target which will perform
linting over the webassembly code using the `wasm32-unknown-unknown`
target, and linting over the code (including tests) using `RUST_TARGET`
defined in the `Makefile` or passed through env.
