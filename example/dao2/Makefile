.POSIX:

SRC = $(shell find src -type f)
WASM_SRC = $(shell find contract -type f)

# Cargo binary
CARGO = cargo

# Contract WASM binaries
WASM_BIN = \
	dao_contract.wasm \
	money_contract.wasm

# Host binaries
BIN = dao

all: $(WASM_BIN) $(BIN)
	@./dao

dao: $(WASM_SRC) $(SRC)
	$(CARGO) build --release --bin $@
	cp -f target/release/$@ $@

dao_contract.wasm: $(WASM_SRC)
	$(CARGO) build --release --package dao-contract --target wasm32-unknown-unknown
	cp -f target/wasm32-unknown-unknown/release/$@ $@

money_contract.wasm: $(WASM_SRC)
	$(CARGO) build --release --package money-contract --target wasm32-unknown-unknown
	cp -f target/wasm32-unknown-unknown/release/$@ $@
	
test: all
	$(CARGO) test --release -- --nocapture

clean:
	rm -f $(BIN) $(WASM_BIN)

.PHONY: all test clean
