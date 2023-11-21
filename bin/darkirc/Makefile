.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo

# Compile target
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)

SRC = \
	Cargo.toml \
	../../Cargo.toml \
	$(shell find src -type f -name '*.rs') \
	$(shell find ../../src -type f -name '*.rs') \

BIN = ../../darkirc

ZKAS = ../../zkas

ZKSRC = $(shell find proof -type f -name '*.zk')
ZKBIN = $(ZKSRC:=.bin)

all: $(BIN)

$(ZKBIN): $(ZKAS) $(ZKSRC)
	$(ZKAS) $(basename $@) -o $@

$(BIN): $(ZKBIN) $(SRC)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --target=$(RUST_TARGET) --release --package darkirc
	cp -f ../../target/$(RUST_TARGET)/release/darkirc $@

android:
	docker build -t test:latest . --file android.Dockerfile
	# Use this command to get an interactive terminal inside docker:
	#docker run -v $(shell pwd)/../../:/root/src -it test:latest /bin/bash
	docker run --rm -v $(shell pwd)/../../:/root/src -t test:latest make _aarch64-android
	cp -f ../../target/aarch64-linux-android/release/darkirc darkirc.aarch64-android

# Invoked inside docker by the command above
# We need to mount this directory as a volume using -v so docker can access it
_aarch64-android:
	cargo build --release --target aarch64-linux-android --package darkirc

clean:
	rm -f $(BIN)

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BIN) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/darkirc

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/darkirc

.PHONY: all clean install uninstall
