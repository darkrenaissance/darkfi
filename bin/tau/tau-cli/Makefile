.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo

SRC = \
	Cargo.toml \
	../../../Cargo.toml \
	$(shell find src -type f) \
	$(shell find ../../../src -type f) \

BIN = ../../../tau

all: $(BIN)

$(BIN): $(SRC)
	$(CARGO) build $(TARGET_PRFX)$(RUST_TARGET) --release --package tau
	cp -f ../../../target/$(RUST_TARGET)/release/tau $@

clean:
	rm -f $(BIN)

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BIN) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/tau

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/tau

.PHONY: all clean install uninstall
