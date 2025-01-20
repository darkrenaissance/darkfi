.POSIX:

APK = target/android-artifacts/release/apk/darkwallet.apk

SRC = \
	build.rs \
	Cargo.lock \
	Cargo.toml \
	Dockerfile \
	$(shell find assets -type f) \
	$(shell find src -type f) \
	$(shell find res -type f)

#FEATURES = --features=emulate-android
#FEATURES = --features=enable-plugins

all: $(SRC) fonts
	# You can either install cargo-limit or just s/lbuild/build
	# `cargo install cargo-limit`
	cargo lbuild $(FEATURES)
	-mv target/debug/darkwallet .
	./darkwallet

darkwallet.apk: $(SRC) fonts
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/darkwallet/ -t apk cargo quad-apk build --release
	cp -f $(APK) $@

ibm-plex-mono-regular.otf:
	wget -c https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/ibm-plex-mono-regular.otf

NotoColorEmoji.ttf:
	wget -c https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/NotoColorEmoji.ttf

fonts: ibm-plex-mono-regular.otf NotoColorEmoji.ttf

android: $(SRC) fonts
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/darkwallet/ -t apk cargo quad-apk build --release
	adb uninstall darkfi.darkwallet
	adb install $(APK)
	reset
	adb logcat -c
	adb logcat -v color -s darkfi -s SAPP -s libc -s DEBUG -s ActivityManager -s ActivityTaskManager -s WindowManager

# Useful for dev
cli:
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/darkwallet/ -it apk bash

clean:
	podman run -v $(shell pwd):/root/dw -w /root/dw -t apk rm -fr target/
	rm -f darkwallet.apk

.PHONY: all android cli clean
