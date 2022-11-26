.POSIX:

# Install prefix
PREFIX = /usr/local

# Cargo binary
CARGO = cargo

# Flags passed to cargo/rustc
#RUSTFLAGS = -C target-cpu=native

# Binaries to be built
BINS = drk darkfid tau taud ircd dnetview darkotc darkwikid darkwiki dao daod

# Common dependencies which should force the binaries to be rebuilt
BINDEPS = \
	Cargo.toml \
	$(shell find bin/*/src -type f) \
	$(shell find bin -type f -name '*.toml') \
	$(shell find src -type f) \
	$(shell find contrib/token -type f)

# ZK proofs to compile with zkas
PROOFS = \
	$(shell find bin/dao/daod/proof -type f -name '*.zk') \
	$(shell find example/dao/proof -type f -name '*.zk') \
	$(shell find proof -type f -name '*.zk') \
	example/simple.zk

PROOFS_BIN = $(PROOFS:=.bin)

all: zkas $(PROOFS_BIN) $(BINS)

zkas: $(BINDEPS)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --all-features --release --package $@
	cp -f target/release/$@ $@

contracts: zkas
	$(MAKE) -C src/contract/money

$(PROOFS_BIN): $(PROOFS)
	./zkas $(basename $@) -o $@

token_lists:
	$(MAKE) -C contrib/token all

$(BINS): token_lists contracts $(PROOFS_BIN) $(BINDEPS)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --all-features --release --package $@
	cp -f target/release/$@ $@

check: token_lists zkas $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) hack check --release --feature-powerset --all

fix: token_lists zkas $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --release --all-features --fix --allow-dirty --all

clippy: token_lists zkas $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --release --all-features --all

rustdoc: token_lists zkas
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) doc --release --workspace --all-features \
		--no-deps --document-private-items

test: token_lists zkas $(PROOFS_BIN) test-tx
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) test --release --all-features --all

test-tx: zkas
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) run --release --features=node,zkas --example tx

test-dao: zkas
	$(MAKE) -C example/dao

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

.PHONY: all contracts check fix clippy rustdoc test test-tx clean cleanbin install uninstall
