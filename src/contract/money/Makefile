.POSIX:

# Cargo binary
RUST_VER = +nightly-2023-06-21
CARGO = cargo $(RUST_VER)

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
WASM_BIN = money_contract.wasm

# Just compile the tests
NO_RUN = "--no-run"

all: $(WASM_BIN)

$(WASM_BIN): $(WASM_SRC) $(PROOFS_BIN)
	$(CARGO) build --release --package darkfi-money-contract --target wasm32-unknown-unknown
	cp -f ../../../target/wasm32-unknown-unknown/release/darkfi_money_contract.wasm $@

$(PROOFS_BIN): $(ZKAS) $(PROOFS_SRC)
	$(ZKAS) $(basename $@) -o $@

test-integration: all
	$(CARGO) test --release --features=no-entrypoint,client \
		--package darkfi-money-contract \
		--test integration $(ARGS)

test-mint-pay-swap: all
	$(CARGO) test --release --features=no-entrypoint,client \
		--package darkfi-money-contract \
		--test mint_pay_swap $(ARGS)

test-txs-verification: all
	$(CARGO) test --release --features=no-entrypoint,client \
		--package darkfi-money-contract \
		--test txs_verification $(ARGS)

test-genesis-mint: all
	$(CARGO) test --release --features=no-entrypoint,client \
		--package darkfi-money-contract \
		--test genesis_mint $(ARGS)

bench:
	$(CARGO) test --release --features=no-entrypoint,client \
		--package darkfi-money-contract \
		--test verification_bench $(FILTER)

test: test-integration test-mint-pay-swap test-txs-verification test-genesis-mint

test-no-run:
	$(MAKE) test-integration ARGS=$(NO_RUN)
	$(MAKE) test-mint-pay-swap ARGS=$(NO_RUN)
	$(MAKE) test-txs-verification ARGS=$(NO_RUN)
	$(MAKE) test-genesis-mint ARGS=$(NO_RUN)

clean:
	rm -f $(PROOFS_BIN) $(WASM_BIN)

.PHONY: all test-integration test-mint-pay-swap test-txs-verification test-genesis-mint bench test clean
