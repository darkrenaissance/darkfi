.POSIX:

# Install prefix
PREFIX = /usr/local

# Cargo binary
CARGO = cargo

# Binaries to be built
BINS = zkas drk darkfid gatewayd

# Common dependencies which should force the binaries to be rebuilt
BINDEPS = \
	Cargo.toml \
	$(shell find bin/*/src -type f) \
	$(shell find bin -type f -name '*.toml') \
	$(shell find src -type f) \
	$(shell find script/sql -type f) \
	$(shell find contrib/token -type f)

all: $(BINS)

token_lists:
	$(MAKE) -C contrib/token all

$(BINS): token_lists $(BINDEPS)
	$(CARGO) build --all-features --release --package $@
	cp -f target/release/$@ $@

check:
	$(CARGO) hack check --release --feature-powerset --all

fix:
	$(CARGO) clippy --release --all-features --fix --allow-dirty --all

clippy:
	$(CARGO) clippy --release --all-features --all

# zkas source files which we want to compile for tests
VM_SRC = proof/arithmetic.zk proof/mint.zk proof/burn.zk
VM_BIN = $(VM_SRC:=.bin)

$(VM_BIN): zkas $(VM_SRC)
	./zkas $(basename $@) -o $@

test: $(VM_BIN) test-tx
	$(CARGO) test --release --all-features --all

test-tx:
	$(CARGO) run --release --features=node,zkas --example tx

clean:
	rm -f $(BINS)

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BINS) $(DESTDIR)$(PREFIX)/bin

uninstall:
	for i in $(BINS); \
	do \
		rm -f $(DESTDIR)$(PREFIX)/bin/$$i; \
	done;

.PHONY: all check fix clippy test test-tx clean install uninstall
