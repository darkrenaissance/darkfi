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
	$(shell find ../../sdk -type f -name '*.rs') \
	$(shell find ../../serial -type f -name '*.rs')

# wasm contract binary
WASM_BIN = deployooor_contract.wasm

all: $(WASM_BIN)

$(WASM_BIN): $(WASM_SRC) $(PROOFS_BIN)
	$(CARGO) build --release --package darkfi-deployooor-contract --target wasm32-unknown-unknown
	cp -f ../../../target/wasm32-unknown-unknown/release/darkfi_deployooor_contract.wasm $@

$(PROOFS_BIN): $(ZKAS) $(PROOFS_SRC)
	$(ZKAS) $(basename $@) -o $@

clean:
	rm -f $(PROOFS_BIN) $(WASM_BIN)

.PHONY: all clean
