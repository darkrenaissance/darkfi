.POSIX:

# Cargo binary
CARGO = cargo

# Binary to be built
BIN = ircd

# Dependencies which should force the binaries to be rebuilt
BINDEPS = \
	Cargo.toml \
	$(shell find src -type f) \
	$(shell find ../../sql -type f) \
	$(shell find ../../contrib/token -type f)

all: $(BIN)

$(BIN): $(BINDEPS)
	$(CARGO) build --release --all-features
	cp -f target/release/$@ $@

check: $(BINDEPS)
	$(CARGO) hack check --release --feature-powerset --no-dev-deps

fix:
	$(CARGO) clippy --release --all-features --fix --allow-dirty

clippy:
	$(CARGO) clippy --release --all-features

test:
	$(CARGO) test --release --all-features

clean:
	rm -f $(BIN)

.PHONY: all check fix clippy test clean
