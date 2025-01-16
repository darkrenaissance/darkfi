.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo +nightly

# Compile target
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)
# Uncomment when doing musl static builds
#RUSTFLAGS = -C target-feature=+crt-static -C link-self-contained=yes

SRC = \
	Cargo.toml \
	../../../Cargo.toml \
	$(shell find src -type f -name '*.rs') \
	$(shell find ../../../src -type f -name '*.rs') \

BIN = $(shell grep '^name = ' Cargo.toml | sed 1q | cut -d' ' -f3 | tr -d '"')

all: $(BIN) bundle_contracts_src

$(BIN): $(SRC)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --target=$(RUST_TARGET) --release --package $@
	# TODO: Update target path to darkfi project workspace once explorer is transitioned to bin
	cp -f target/$(RUST_TARGET)/release/$@ $@
	cp -f target/$(RUST_TARGET)/release/$@ ../../../$@

clean:
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean --target=$(RUST_TARGET) --release --package $(BIN)
	rm -f $(BIN) ../../../$(BIN)
	rm -rf native_contracts_src

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BIN) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/$(BIN)

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/$(BIN)

bundle_contracts_src:
	@mkdir -p $(CURDIR)/native_contracts_src
	@(cd ../../../src && \
		tar -cf $(CURDIR)/native_contracts_src/deployooor_contract_src.tar -C contract/deployooor/src --transform 's,^./,,' . && \
		tar -cf $(CURDIR)/native_contracts_src/dao_contract_src.tar -C contract/dao/src --transform 's,^./,,' . 2>/dev/null && \
		find contract/dao/proof -name '*.zk' -exec tar -rf $(CURDIR)/native_contracts_src/dao_contract_src.tar --transform 's,^.*proof/,proof/,' {} + && \
		tar -cf $(CURDIR)/native_contracts_src/money_contract_src.tar -C contract/money/src --transform 's,^./,,' . && \
		find contract/money/proof -name '*.zk' -exec tar -rf $(CURDIR)/native_contracts_src/money_contract_src.tar --transform 's,^.*proof/,proof/,' {} + \
	)

.PHONY: all clean install uninstall bundle_contracts_src