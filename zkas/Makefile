.POSIX:

# Cargo binary
CARGO = cargo

BINS = zkas

# Dependencies which should force the binaries to be rebuilt
BINDEPS = \
	Cargo.toml \
	$(shell find src -type f)

all: $(BINS)

$(BINS): $(BINDEPS)
	$(CARGO) build --release --all-features --bin $@
	cp -f ../target/release/$@ $@

test:
	$(CARGO) test --release --all-features

fix:
	$(CARGO) clippy --release --all-features --fix --allow-dirty

clippy:
	$(CARGO) clippy --release --all-features

clean:
	rm -f $(BINS)

distclean: clean
	rm -rf target

.PHONY: all test fix clippy clean distclean
