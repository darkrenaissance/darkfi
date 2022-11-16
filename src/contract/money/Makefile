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

all: $(ZKAS_BIN) $(WASM_BIN)

$(ZKAS_BIN): $(ZKAS_SRC)
	$(ZKAS) $(basename $@) -o $@

money_contract.wasm: $(ZKAS_BIN) $(WASM_SRC)
	$(CARGO) build --release --package darkfi-money-contract --target wasm32-unknown-unknown
	cp -f ../../../target/wasm32-unknown-unknown/release/darkfi_money_contract.wasm $@

test:

clean:
	rm -f $(ZKAS_BIN) $(WASM_BIN)

# We always rebuild the wasm no matter what
.PHONY: all test clean
