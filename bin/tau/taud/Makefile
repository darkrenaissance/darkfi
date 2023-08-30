.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo +nightly

SRC = \
	Cargo.toml \
	../../../Cargo.toml \
	$(shell find src -type f) \
	$(shell find ../../../src -type f) \

BIN = ../../../taud

all: $(BIN)

$(BIN): $(SRC)
	$(CARGO) build $(TARGET_PRFX)$(RUST_TARGET) --release --package taud
	cp -f ../../../target/$(RUST_TARGET)/release/taud $@

clean:
	rm -f $(BIN)

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BIN) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/taud

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/taud

.PHONY: all clean install uninstall
