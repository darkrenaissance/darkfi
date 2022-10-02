.POSIX:

SRC = $(shell find src -type f)

# Cargo binary
CARGO = cargo

BIN = smart_contract.wasm

all: $(BIN)

smart_contract.wasm: $(SRC)
	$(CARGO) build --release --lib --target wasm32-unknown-unknown
	cp -f target/wasm32-unknown-unknown/release/$@ $@

test: all
	$(CARGO) test --release -- --nocapture

clean:
	rm -f $(BIN)

.PHONY: all test clean
