# Cargo binary
CARGO ?= cargo

# Zkas binary
ZKAS ?= ../../../zkas

# zkas circuit source files
ZKAS_SRC = $(shell find proof -type f -name '*.zk')

# wasm source files
WASM_SRC = \
	$(shell find src -type f) \
	$(shell find ../../sdk -type f) \
	$(shell find ../../serial -type f)

# zkas circuit bin files
ZKAS_BIN = $(ZKAS_SRC:=.bin)

# Contract WASM binaries
WASM_BIN = money_contract.wasm

# Filter test to run
FILTER = ''

all: $(ZKAS_BIN) $(WASM_BIN)

$(ZKAS_BIN): $(ZKAS_SRC)
	$(ZKAS) $(basename $@) -o $@

money_contract.wasm: $(ZKAS_BIN) $(WASM_SRC)
	$(CARGO) build --release --package darkfi-money-contract --target wasm32-unknown-unknown
	cp -f ../../../target/wasm32-unknown-unknown/release/darkfi_money_contract.wasm $@

test-drop-pay-swap: all
	$(CARGO) test --release --features=no-entrypoint,client --package darkfi-money-contract --test drop_pay_swap

test: test-drop-pay-swap

bench:
	$(CARGO) test --release --features=no-entrypoint,client \
		--package darkfi-money-contract \
		--test verification_bench $(FILTER)

clean:
	rm -f $(ZKAS_BIN) $(WASM_BIN)

.PHONY: all test clean
