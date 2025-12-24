.POSIX:

# Cargo binary
CARGO = cargo

# wasm build target
WASM_TARGET = wasm32-unknown-unknown

# Cargo package name
PKGNAME = $(shell grep '^name = ' Cargo.toml | cut -d' ' -f3 | tr -d '"')
# wasm contract binary
WASM_BIN = $(PKGNAME:=.wasm)

# zkas compiler binary
ZKAS = "../../zkas"

# zkas circuits
PROOFS_SRC = $(shell find proof -type f -name '*.zk')
PROOFS_BIN = $(PROOFS_SRC:=.bin)

# wasm source files
WASM_SRC = \
	Cargo.toml \
	$(shell find src -type f -name '*.rs')

all: $(WASM_BIN)

$(PROOFS_BIN): $(PROOFS_SRC)
	$(ZKAS) $(basename $@) -o $@

$(WASM_BIN): $(WASM_SRC) $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --target=$(WASM_TARGET) \
		--release --package $(PKGNAME)
	cp -f target/$(WASM_TARGET)/release/$@ $@

clippy: all
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(WASM_TARGET) \
		--release --package $(PKGNAME)

clean:
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean --target=$(WASM_TARGET) \
		--release --package $(PKGNAME)
	rm -f $(PROOFS_BIN) $(WASM_BIN)

.PHONY: all clippy clean
