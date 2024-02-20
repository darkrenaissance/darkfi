.POSIX:

# Cargo binary
CARGO = cargo +nightly

# Compile target for system binaries
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)
# Uncomment when doing musl static builds
#RUSTFLAGS = -C target-feature=+crt-static -C link-self-contained=yes

# wasm build target
WASM_TARGET = wasm32-unknown-unknown

# Cargo package name
PKGNAME = $(shell grep '^name = ' Cargo.toml | cut -d' ' -f3 | tr -d '"')
# wasm contract binary
WASM_BIN = $(PKGNAME:=.wasm)

# wasm source files
WASM_SRC = \
	Cargo.toml \
	../../../Cargo.toml \
	../../../src/sdk/Cargo.toml \
	../../../src/serial/Cargo.toml \
	$(shell find src -type f -name '*.rs') \
	$(shell find ../../sdk -type f -name '*.rs') \
	$(shell find ../../serial -type f -name '*.rs')

all: $(WASM_BIN)

$(WASM_BIN): $(WASM_SRC)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --target=$(WASM_TARGET) \
		--release --package $(PKGNAME)
	cp -f ../../../target/$(WASM_TARGET)/release/$@ $@
	wasm-strip $@

test-integration: all
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) test --target=$(RUST_TARGET) \
		--release --package $(PKGNAME) \
		--features=no-entrypoint,client \
		--test integration

test: test-integration

clippy: all
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(WASM_TARGET) \
		--release --package $(PKGNAME)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(RUST_TARGET) \
		--release --package $(PKGNAME) \
		--features=no-entrypoint,client --tests

clean:
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean --target=$(WASM_TARGET) \
		--release --package $(PKGNAME)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean --target=$(RUST_TARGET) \
		--release --package $(PKGNAME)
	rm -f $(WASM_BIN)

.PHONY: all test-integration test clippy clean
