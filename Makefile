.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo

# Compile target for system binaries
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)
# Uncomment when doing musl static builds
#RUSTFLAGS = -C target-feature=+crt-static -C link-self-contained=yes
# If building natively, this might give you more speed
#RUSTFLAGS = -C target_cpu=native

# List of zkas circuits to compile, used for tests
PROOFS_SRC = \
	$(shell find proof -type f -name '*.zk') \
	$(shell find bin/darkirc/proof -type f -name '*.zk')

PROOFS_BIN = $(PROOFS_SRC:=.bin)

# List of all binaries built
BINS = \
	zkas \
	darkfid \
	minerd \
	drk \
	darkirc \
	genev \
	genevd \
	lilith \
	taud \
	vanityaddr \
	explorerd \
	fud \
	fu

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

minerd: contracts
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

explorerd:
	$(MAKE) -C bin/explorer/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

explorerd_bundle_contracts_src: contracts
	$(MAKE) -C bin/explorer/explorerd bundle_contracts_src

fud:
	$(MAKE) -C bin/fud/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

fu:
	$(MAKE) -C bin/fud/$@ \
		PREFIX="$(PREFIX)" \
		CARGO="$(CARGO)" \
		RUST_TARGET="$(RUST_TARGET)" \
		RUSTFLAGS="$(RUSTFLAGS)"

# -- END OF BINS --

fmt:
	$(CARGO) +nightly fmt --all

# cargo install cargo-hack
check: explorerd_bundle_contracts_src $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) hack check --target=$(RUST_TARGET) \
		--release --feature-powerset --workspace

clippy: explorerd_bundle_contracts_src $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(RUST_TARGET) \
		--release --all-features --workspace --tests

fix: explorerd_bundle_contracts_src $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(RUST_TARGET) \
		--release --all-features --workspace --tests --fix --allow-dirty

rustdoc: explorerd_bundle_contracts_src $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) doc --target=$(RUST_TARGET) \
		--release --all-features --workspace --document-private-items --no-deps

test: explorerd_bundle_contracts_src $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) test --target=$(RUST_TARGET) \
		--release --all-features --workspace

bench-zk-from-json: explorerd_bundle_contracts_src $(PROOFS_BIN)
	rm -f src/contract/test-harness/*.bin
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) bench --target=$(RUST_TARGET) \
		--bench zk_from_json --all-features --workspace \
		-- --save-baseline master

bench: explorerd_bundle_contracts_src $(PROOFS_BIN)
	rm -f src/contract/test-harness/*.bin
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) bench --target=$(RUST_TARGET) \
		--all-features --workspace \
		-- --save-baseline master

coverage: explorerd_bundle_contracts_src $(PROOFS_BIN)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) llvm-cov --target=$(RUST_TARGET) \
		--release --all-features --workspace --html

clean:
	$(MAKE) -C src/contract/money clean
	$(MAKE) -C src/contract/dao clean
	$(MAKE) -C src/contract/deployooor clean
	$(MAKE) -C bin/zkas clean
	$(MAKE) -C bin/darkfid clean
	$(MAKE) -C bin/minerd clean
	$(MAKE) -C bin/drk clean
	$(MAKE) -C bin/darkirc clean
	$(MAKE) -C bin/genev/genev-cli clean
	$(MAKE) -C bin/genev/genevd clean
	$(MAKE) -C bin/lilith clean
	$(MAKE) -C bin/tau/taud clean
	$(MAKE) -C bin/vanityaddr clean
	$(MAKE) -C bin/explorer/explorerd clean
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean --target=$(RUST_TARGET) --release
	rm -f $(PROOFS_BIN)

distclean: clean
	rm -rf target

.PHONY: all $(BINS) explorerd_bundle_contracts_src fmt check clippy fix rustdoc \
	test bench-zk-from-json bench coverage clean distclean
