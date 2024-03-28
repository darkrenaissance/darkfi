.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo +nightly

# Compile target for system binaries
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)
# Uncomment when doing musl static builds
#RUSTFLAGS = -C target-feature=+crt-static -C link-self-contained=yes

# List of zkas circuits to compile, used for tests
PROOFS_SRC = $(shell find proof -type f -name '*.zk')
PROOFS_BIN = $(PROOFS_SRC:=.bin)

# List of all binaries built
BINS = \
	zkas \
	darkfid \
	minerd \
	darkfi-mmproxy \
	darkirc \
	genev \
	genevd \
	lilith \
	taud \
	vanityaddr

all: $(BINS)

zkas:
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

$(PROOFS_BIN): zkas $(PROOFS_SRC)
	./zkas $(basename $@) -o $@

contracts: zkas
	$(MAKE) -C src/contract/money
	$(MAKE) -C src/contract/dao
	$(MAKE) -C src/contract/deployooor

darkfid: contracts
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

minerd:
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

darkfi-mmproxy:
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

drk: contracts
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

darkirc: zkas
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

genev:
	$(MAKE) -C bin/genev/genev-cli \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

genevd:
	$(MAKE) -C bin/genev/genevd \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

lilith:
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

taud:
	$(MAKE) -C bin/tau/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

vanityaddr:
	$(MAKE) -C bin/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

# -- END OF BINS --

fmt:
	$(CARGO) fmt --all

check: contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) hack check --target=$(RUST_TARGET) \
		--release --feature-powerset --workspace

clippy: contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(RUST_TARGET) \
		--release --all-features --workspace --tests

fix: contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(RUST_TARGET) \
		--release --all-features --workspace --tests --fix --allow-dirty

rustdoc: contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) doc --target=$(RUST_TARGET) \
		--release --all-features --workspace --document-private-items --no-deps

test: contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) test --target=$(RUST_TARGET) \
		--release --all-features --workspace

bench_zk-from-json: contracts $(PROOFS_BIN)
	rm src/contract/test-harness/*.bin
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) bench --target=$(RUST_TARGET) \
		--bench zk_from_json --all-features --workspace \
		-- --save-baseline master

bench: contracts $(PROOFS_BIN)
	rm src/contract/test-harness/*.bin
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) bench --target=$(RUST_TARGET) \
		--all-features --workspace \
		-- --save-baseline master

coverage: contracts $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) llvm-cov --target=$(RUST_TARGET) \
		--release --all-features --workspace --html

clean:
	$(MAKE) -C src/contract/money clean
	$(MAKE) -C src/contract/dao clean
	$(MAKE) -C src/contract/deployooor clean
	$(MAKE) -C bin/zkas clean
	$(MAKE) -C bin/darkfid clean
	$(MAKE) -C bin/minerd clean
	$(MAKE) -C bin/darkfi-mmproxy clean
	$(MAKE) -C bin/darkirc clean
	$(MAKE) -C bin/genev/genev-cli clean
	$(MAKE) -C bin/genev/genevd clean
	$(MAKE) -C bin/lilith clean
	$(MAKE) -C bin/tau/taud clean
	$(MAKE) -C bin/vanityaddr clean
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean --target=$(RUST_TARGET) --release
	rm -f $(PROOFS_BIN)

distclean: clean
	rm -rf target

.PHONY: all $(BINS) fmt check clippy fix rustdoc test coverage clean distclean
