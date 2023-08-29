.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo +nightly

SRC = \
	Cargo.toml \
	../../Cargo.toml \
	$(shell find src -type f) \
	$(shell find ../../src/serial -type f) \
	$(shell find ../../src/zkas -type f)

BIN = ../../zkas

all: $(BIN)

$(BIN): $(SRC)
	$(CARGO) build $(TARGET_PRFX)$(RUST_TARGET) --release --package zkas
	cp -f ../../target/$(RUST_TARGET)/release/zkas $@

clean:
	rm -f $(BIN)

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BIN) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/zkas

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/zkas

.PHONY: all clean install uninstall
