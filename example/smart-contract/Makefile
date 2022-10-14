.POSIX:

WASM_SRC = $(shell find src -type f)
PROOF_SRC = $(shell find proof -type f -name '*.zk')

# Cargo binary
CARGO = cargo

# zkas binary
ZKAS = ../../zkas

# wasm-strip binary (Part of https://github.com/WebAssembly/wabt)
WASM_STRIP = wasm-strip

# Contract WASM binary
WASM_BIN = contract.wasm

# ZK circuit binaries
PROOF_BIN = $(PROOF_SRC:=.bin)

all: $(PROOF_BIN) $(WASM_BIN)

strip: $(WASM_BIN)
	$(WASM_STRIP) $<

$(WASM_BIN): $(WASM_SRC)
	$(CARGO) build --release --lib --target wasm32-unknown-unknown
	cp -f target/wasm32-unknown-unknown/release/*.wasm $@

$(PROOF_BIN): $(PROOF_SRC)
	$(ZKAS) $(basename $@) -o $@

test: all
	$(CARGO) test --release -- --nocapture

clean:
	rm -f $(PROOF_BIN) $(WASM_BIN)

.PHONY: all test clean
