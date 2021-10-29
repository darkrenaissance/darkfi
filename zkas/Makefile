.POSIX:

CARGO = cargo

BIN = zkas

# Dependencies which should force the binaries to be rebuilt
BINDEPS = \
	Cargo.toml \
	$(shell find src -type f)

all: $(BIN)

$(BIN): $(BINDEPS)
	$(CARGO) build --release --all-features --bin $@
	cp target/release/$@ $@

test:
	$(CARGO) test --release --all-features

fix:
	$(CARGO) fix --release --all-features --allow-dirty

clippy:
	$(CARGO) clippy --release --all-features

clean:
	rm -f $(BIN)

distclean: clean
	rm -rf target

.PHONY: all test fix clippy clean distclean
