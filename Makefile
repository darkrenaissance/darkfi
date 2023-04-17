.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo

# Flags passed to cargo/rustc
#RUSTFLAGS = -C target-cpu=native

# Optional compile target
#RUST_TARGET = x86_64-unknown-linux-musl
# Uncomment this if the above is uncommented
#TARGET_PRFX = --target=

# Binaries to be built
BINS = drk darkfid ircd dnetview faucetd vanityaddr

# zkas dependencies
ZKASDEPS = \
	Cargo.toml \
	bin/zkas/Cargo.toml \
	$(shell find src/zkas -type f) \
	$(shell find src/serial -type f) \
	$(shell find bin/zkas/src -type f)

# ZK proofs to compile with zkas
PROOFS_SRC = $(shell find proof -type f -name '*.zk') example/simple.zk
PROOFS_BIN = $(PROOFS_SRC:=.bin)

# Common dependencies which should force the binaries to be rebuilt
BINDEPS = \
	Cargo.toml \
	$(shell find bin/*/src -type f) \
	$(shell find bin -type f -name '*.toml') \
	$(shell find src -type f) \
	$(shell find contrib/token -type f)

all: $(BINS)

zkas: $(ZKASDEPS)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build $(TARGET_PRFX)$(RUST_TARGET) \
		--all-features --release --package $@
	cp -f target/$(RUST_TARGET)/release/$@ $@

$(PROOFS_BIN): zkas $(PROOFS_SRC)
	./zkas $(basename $@) -o $@

contracts: zkas
	$(MAKE) -C src/contract/money
	$(MAKE) -C src/contract/dao

token_lists:
	$(MAKE) -C contrib/token all

$(BINS): token_lists contracts $(PROOFS_BIN) $(BINDEPS)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build $(TARGET_PRFX)$(RUST_TARGET) \
		--all-features --release --package $@
	cp -f target/$(RUST_TARGET)/release/$@ $@

check: token_lists contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) hack check --release --feature-powerset --all

fix: token_lists contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --release --all-features --fix --allow-dirty --all

clippy: token_lists contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --release --all-features --all

rustdoc: token_lists contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) doc --release --all-features --workspace --document-private-items

test: token_lists $(PROOFS_BIN) contracts
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) test --release --all-features --all

cleanbin:
	rm -f $(BINS)

clean: cleanbin
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean

install:
	@for i in $(BINS); \
	do \
		if test ! -f $$i; \
		then \
			echo "The '$$i' binary was not built."; \
			echo "You should run 'make BINS=$$i' as a normal user before installing."; \
			exit 1; \
		fi; \
	done;
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BINS) $(DESTDIR)$(PREFIX)/bin

uninstall:
	for i in $(BINS); \
	do \
		rm -f $(DESTDIR)$(PREFIX)/bin/$$i; \
	done;

.PHONY: all contracts token_lists check fix clippy rustdoc test cleanbin clean install uninstall
