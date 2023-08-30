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
BINS = \
	zkas \
	darkfid \
	darkfid2 \
	faucetd \
	darkirc \
	genev \
	genevd \
	lilith \
	tau \
	taud \
	vanityaddr

# ZK proofs to compile with zkas
PROOFS_SRC = $(shell find proof -type f -name '*.zk') example/simple.zk
PROOFS_BIN = $(PROOFS_SRC:=.bin)

all: $(BINS)

zkas:
	$(MAKE) -C bin/zkas

$(PROOFS_BIN): zkas $(PROOFS_SRC)
	./zkas $(basename $@) -o $@

contracts: zkas
	$(MAKE) -C src/contract/money
	$(MAKE) -C src/contract/dao
	$(MAKE) -C src/contract/consensus
	$(MAKE) -C src/contract/deployooor

darkfid: $(PROOFS_BIN) contracts
	$(MAKE) -C bin/darkfid

darkfid2: contracts
	$(MAKE) -C bin/darkfid2

faucetd: contracts
	$(MAKE) -C bin/faucetd

darkirc:
	$(MAKE) -C bin/darkirc

genev:
	$(MAKE) -C bin/genev/genev-cli

genevd:
	$(MAKE) -C bin/genev/genevd

lilith:
	$(MAKE) -C bin/lilith

tau:
	$(MAKE) -C bin/tau/tau-cli

taud:
	$(MAKE) -C bin/tau/taud

vanityaddr:
	$(MAKE) -C bin/vanityaddr

fmt:
	$(CARGO) fmt

check: $(PROOFS_BIN) contracts
	$(CARGO) hack check --release --feature-powerset --workspace

clippy: $(PROOFS_BIN) contracts
	$(CARGO) clippy --release --all-features --workspace --tests

fix: $(PROOFS_BIN) contracts
	$(CARGO) clippy --release --all-features --fix --allow-dirty --workspace

rustdoc: $(PROOFS_BIN) contracts
	$(CARGO) doc --release --all-features --workspace --document-private-items --no-deps

test: $(PROOFS_BIN) contracts
	$(CARGO) test --release --all-features --workspace

coverage: $(PROOFS_BIN) contracts
	$(CARGO) llvm-cov --release --all-features --workspace --html

clean:
	$(MAKE) -C src/contract/money clean
	$(MAKE) -C src/contract/dao clean
	$(MAKE) -C src/contract/consensus clean
	$(MAKE) -C src/contract/deployooor clean
	$(MAKE) -C bin/zkas clean
	$(MAKE) -C bin/darkfid clean
	$(MAKE) -C bin/darkfid2 clean
	$(MAKE) -C bin/faucetd clean
	$(MAKE) -C bin/darkirc clean
	$(MAKE) -C bin/genev/genev-cli clean
	$(MAKE) -C bin/genev/genevd clean
	$(MAKE) -C bin/lilith clean
	$(MAKE) -C bin/tau/tau-cli clean
	$(MAKE) -C bin/tau/taud clean
	$(MAKE) -C bin/vanityaddr clean
	rm -f $(PROOFS_BIN)

distclean: clean
	$(CARGO) clean
	rm -rf target

install: all
	@for i in $(BINS); \
	do \
		$(MAKE) -C $$i install \
	done;

uninstall:
	for i in $(BINS); \
	do \
		$(MAKE) -C $$i uninstall \
	done;

.PHONY: all contracts check fix fmt clippy rustdoc test coverage distclean clean install uninstall $(BINS)
