.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo

# Compile target
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)
# Uncomment when doing musl static builds
#RUSTFLAGS = -C target-feature=+crt-static -C link-self-contained=yes

SRC = \
	Cargo.toml \
	../../../Cargo.toml \
	$(shell find src -type f -name '*.rs') \
	$(shell find ../../../src -type f -name '*.rs') \

BIN = $(shell grep '^name = ' Cargo.toml | cut -d' ' -f3 | tr -d '"')

all: $(BIN)

$(BIN): $(SRC)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --target=$(RUST_TARGET) --release --package $@
	cp -f ../../../target/$(RUST_TARGET)/release/$@ $@
	cp -f ../../../target/$(RUST_TARGET)/release/$@ ../../../$@

clippy: all
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(RUST_TARGET) --release --package $(BIN) --tests

clean:
	rm -f $(BIN) ../../../$(BIN)

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BIN) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/$(BIN)

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/$(BIN)

.PHONY: all clean install uninstall

