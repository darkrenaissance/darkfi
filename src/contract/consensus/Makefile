.POSIX:

# Cargo binary
CARGO = cargo

# zkas compiler binary
ZKAS = ../../../zkas

# zkas circuits
PROOFS_SRC = $(shell find proof -type f -name '*.zk')
PROOFS_BIN = $(PROOFS_SRC:=.bin)

# wasm source files
WASM_SRC = \
	$(shell find src -type f) \
	$(shell find ../../sdk -type f) \
	$(shell find ../../serial -type f)

# wasm contract binary
WASM_BIN = consensus_contract.wasm

all: $(WASM_BIN)

$(WASM_BIN): $(WASM_SRC) $(PROOFS_BIN)
	$(CARGO) build --release --package darkfi-consensus-contract --target wasm32-unknown-unknown
	cp -f ../../../target/wasm32-unknown-unknown/release/darkfi_consensus_contract.wasm $@

$(PROOFS_BIN): $(ZKAS) $(PROOFS_SRC)
	$(ZKAS) $(basename $@) -o $@

test-stake-unstake: all
	$(CARGO) test --release --features=no-entrypoint,client \
		--package darkfi-consensus-contract \
		--test stake_unstake

test: test-stake-unstake

clean:
	rm -f $(PROOFS_BIN) $(WASM_BIN)

.PHONY: all test-stake-unstake test clean
