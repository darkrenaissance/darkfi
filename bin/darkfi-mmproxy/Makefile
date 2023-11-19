.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo +nightly

# Compile target
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut  -d' ' -f2)

SRC = \
	Cargo.toml \
	../../Cargo.toml \
	$(shell find src -type f -name '*.rs') \
	$(shell find ../../src -type f -name '*.rs') \

BIN = ../../darkfi-mmproxy

all: $(BIN)

$(BIN): $(SRC)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --target=$(RUST_TARGET) --release --package darkfi-mmproxy
	cp -f ../../target/$(RUST_TARGET)/release/darkfi-mmproxy $@

clean:
	rm -f $(BIN)

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BIN) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/darkfi-mmproxy

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/darkfi-mmproxy

.PHONY: all clean install uninstall
