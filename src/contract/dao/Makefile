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
WASM_BIN = dao_contract.wasm

# Just compile the tests
NO_RUN = "--no-run"

all: $(WASM_BIN)

$(WASM_BIN): $(WASM_SRC) $(PROOFS_BIN)
	$(CARGO) build --release --package darkfi-dao-contract --target wasm32-unknown-unknown
	cp -f ../../../target/wasm32-unknown-unknown/release/darkfi_dao_contract.wasm $@

$(PROOFS_BIN): $(ZKAS) $(PROOFS_SRC)
	$(ZKAS) $(basename $@) -o $@

test-integration: all
	$(CARGO) test --release --features=no-entrypoint,client \
		--package darkfi-dao-contract \
		--test integration $(ARGS)

test: test-integration

test-no-run:
	$(MAKE) test-integration ARGS=$(NO_RUN)

clean:
	rm -f $(PROOFS_BIN) $(WASM_BIN)

.PHONY: all test test-integration test-no-run clean
