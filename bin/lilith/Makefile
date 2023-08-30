.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo +nightly

SRC = \
	Cargo.toml \
	../../Cargo.toml \
	$(shell find src -type f) \
	$(shell find ../../src -type f) \

BIN = ../../lilith

all: $(BIN)

$(BIN): $(SRC)
	$(CARGO) build $(TARGET_PRFX)$(RUST_TARGET) --release --package lilith
	cp -f ../../target/$(RUST_TARGET)/release/lilith $@

clean:
	rm -f $(BIN)

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BIN) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/lilith

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/lilith

.PHONY: all clean install uninstall
