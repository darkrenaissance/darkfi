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

all: $(SRC) fonts
	cargo lbuild
	mv target/debug/darkwallet .
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
	adb logcat -s darkfi

# Useful for dev
cli:
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/darkwallet/ -it apk bash

clean:
	podman run -v $(shell pwd):/root/dw -w /root/dw -t apk rm -fr target/
	rm -f darkwallet.apk

.PHONY: all android cli clean
