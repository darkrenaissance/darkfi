.POSIX:

SRC = $(shell find src -type f)

# Cargo binary
CARGO = cargo

DEPS = smart_contract.wasm

all: $(DEPS)

smart_contract.wasm: $(SRC)
	$(CARGO) build --release --lib --target wasm32-unknown-unknown
	cp -f target/wasm32-unknown-unknown/release/$@ $@

.PHONY: all
