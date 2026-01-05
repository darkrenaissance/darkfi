.POSIX:

# Install prefix
PREFIX = $(HOME)/.cargo

# Cargo binary
CARGO = cargo

# Compile target
RUST_TARGET = $(shell rustc -Vv | grep '^host: ' | cut -d' ' -f2)
# Uncomment when doing musl static builds
#RUSTFLAGS = -C target-feature=+crt-static -C link-self-contained=yes

# zkas compiler binary
ZKAS = ../../zkas

# zkas circuits
PROOFS_SRC = $(shell find ../../src/event_graph -type f -name '*.zk')
PROOFS_BIN = $(PROOFS_SRC:=.bin)

SRC = \
	Cargo.toml \
	../../Cargo.toml \
	$(shell find src -type f -name '*.rs') \
	$(shell find ../../src -type f -name '*.rs') \

BIN = $(shell grep '^name = ' Cargo.toml | sed 1q | cut -d' ' -f3 | tr -d '"')

all: $(BIN)

$(PROOFS_BIN): $(ZKAS) $(PROOFS_SRC)
	$(ZKAS) $(basename $@) -o $@

$(BIN): $(PROOFS_BIN) $(SRC)
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) build --target=$(RUST_TARGET) --release --package $@ --bin $@
	cp -f ../../target/$(RUST_TARGET)/release/$@ $@
	cp -f ../../target/$(RUST_TARGET)/release/$@ ../../$@

# To build for android, install the Android NDK (Android Studio)
# and then install cargo-ndk: `cargo install cargo-ndk`.
# After that, add new android rust toolchains:
# - rustup target add aarch64-linux-android
# - rustup target add armv7-linux-androideabi
# - rustup target add x86_64-linux-android
# - rustup target add i686-linux-android
$(BIN).android64:
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) ndk -t arm64-v8a -o ./jniLibs build --release --package $(BIN)
	cp -f ../../target/aarch64-linux-android/release/$(BIN) $@

# UNTESTED:
#$(BIN).android32:
#	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) ndk -t armeabi-v7a -o ./jniLibs build --release --package $(BIN)
#	cp -f ../../target/armv7-linux-androideabi/release/$(BIN) $(BIN).$@

# This target doesn't require installing any packages from Android Studio
podman-android:
	# s/podman/docker/ if you are using docker instead
	podman build -t test:latest . --file android.Dockerfile
	# Use this command to get an interactive terminal inside docker:
	#docker run -v $(shell pwd)/../../:/root/src -it test:latest /bin/bash
	podman run --rm -v $(shell pwd)/../../:/root/src -t test:latest make _aarch64-android
	cp -f ../../target/aarch64-linux-android/release/darkirc darkirc.aarch64-android
# Invoked inside docker by the command above
# We need to mount this directory as a volume using -v so docker can access it
_aarch64-android:
	cargo build --release --target aarch64-linux-android --package darkirc

clippy: all
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clippy --target=$(RUST_TARGET) --release --package $(BIN) --tests

clean:
	RUSTFLAGS="$(RUSTFLAGS)" $(CARGO) clean --target=$(RUST_TARGET) --release --package $(BIN)
	rm -f $(BIN) ../../$(BIN) $(BIN).android64 $(BIN).android32

install: all
	mkdir -p $(DESTDIR)$(PREFIX)/bin
	cp -f $(BIN) $(DESTDIR)$(PREFIX)/bin
	chmod 755 $(DESTDIR)$(PREFIX)/bin/$(BIN)

uninstall:
	rm -f $(DESTDIR)$(PREFIX)/bin/$(BIN)

.PHONY: all clean install uninstall
