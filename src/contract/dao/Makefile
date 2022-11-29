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
WASM_BIN = dao_contract.wasm

all: $(WASM_BIN)

$(ZKAS_BIN): $(ZKAS_SRC)
	$(ZKAS) $(basename $@) -o $@

dao_contract.wasm: $(ZKAS_BIN) $(WASM_SRC)
	$(CARGO) build --release --package darkfi-dao-contract --target wasm32-unknown-unknown
	cp -f ../../../target/wasm32-unknown-unknown/release/darkfi_dao_contract.wasm $@

test: all
	$(CARGO) test --release --features=no-entrypoint,client --package darkfi-dao-contract

clean:
	rm -f $(ZKAS_BIN) $(WASM_BIN)

.PHONY: all test clean
