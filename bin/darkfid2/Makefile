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

BIN = ../../darkfid2

all: $(BIN)

$(BIN): $(SRC)
	$(CARGO) build $(TARGET_PRFX)$(RUST_TARGET) --release --package darkfid2
	cp -f ../../target/$(RUST_TARGET)/release/darkfid2 $@

clean:
	rm -f $(BIN)

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BIN) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/darkfid2

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/darkfid2

.PHONY: all clean install uninstall
