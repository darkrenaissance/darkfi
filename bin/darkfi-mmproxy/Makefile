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

BIN = ../../darkfi-mmproxy

all: $(BIN)

$(BIN): $(SRC)
	$(CARGO) build $(TARGET_PRFX)$(RUST_TARGET) --release --package darkfi-mmproxy
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
