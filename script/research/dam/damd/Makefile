.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo

# Compile target
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)
# Uncomment when doing musl static builds
#RUSTFLAGS = -C target-feature=+crt-static -C link-self-contained=yes

SRC = \
	Cargo.toml \
	$(shell find src -type f -name '*.rs') \

BIN = $(shell grep '^name = ' Cargo.toml | cut -d' ' -f3 | tr -d '"')

all: $(BIN)

$(BIN): $(SRC)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --target=$(RUST_TARGET) --release --package $@
	cp -f target/$(RUST_TARGET)/release/$@ $@

fmt:
	$(CARGO) +nightly fmt --all

clippy:
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(RUST_TARGET) \
		--release --all-features --workspace --tests

clean:
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean --target=$(RUST_TARGET) --release --package $(BIN)
	rm -f $(BIN)

.PHONY: all fmt clippy clean
