.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo +nightly

# Optional compile target
#RUST_TARGET = x86_64-unknown-linux-musl
# Uncomment this if the above is uncommented
#TARGET_PRFX = --target=

# Binaries to be built
BINS = darkfid faucetd drk darkirc dnetview vanityaddr

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

all: $(BINS)

zkas: $(ZKASDEPS)
	$(CARGO) build $(TARGET_PRFX)$(RUST_TARGET) --all-features --release --package $@
	cp -f target/$(RUST_TARGET)/release/$@ $@

$(PROOFS_BIN): zkas $(PROOFS_SRC)
	./zkas $(basename $@) -o $@

contracts: zkas
	$(MAKE) -C src/contract/money
	$(MAKE) -C src/contract/dao
	$(MAKE) -C src/contract/consensus
	$(MAKE) -C src/contract/deployooor

$(BINS): contracts $(PROOFS_BIN) $(BINDEPS)
	$(CARGO) build $(TARGET_PRFX)$(RUST_TARGET) --all-features --release --package $@
	cp -f target/$(RUST_TARGET)/release/$@ $@

check: contracts $(PROOFS_BIN)
	$(CARGO) hack check --release --feature-powerset --all

fix: contracts $(PROOFS_BIN)
	$(CARGO) clippy --release --all-features --fix --allow-dirty --all

clippy: contracts $(PROOFS_BIN)
	$(CARGO) clippy --release --all-features --all

rustdoc: contracts $(PROOFS_BIN)
	$(CARGO) doc --release --all-features --workspace --document-private-items

test: $(PROOFS_BIN) contracts
	$(CARGO) test --release --all-features --all

test-no-run: $(PROOFS_BIN) contracts
	$(CARGO) test --release --all-features --all --no-run

coverage: contracts $(PROOFS_BIN)
	$(CARGO) llvm-cov --release --all-features --workspace --html

cleanbin:
	rm -f $(BINS)

clean: cleanbin
	$(CARGO) clean

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

.PHONY: all check fix clippy test test-no-run cleanbin clean \
	install uninstall contracts coverage
