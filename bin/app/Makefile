.POSIX:

APK = target/android-artifacts/release/apk/darkfi-app.apk

SRC = \
	build.rs \
	Cargo.lock \
	Cargo.toml \
	Dockerfile \
	$(shell find assets -type f) \
	$(shell find src -type f)

RELEASE_FEATURES = --features=enable-plugins

#FEATURES = --features=emulate-android
#FEATURES = --features=enable-netdebug,enable-plugins
DEV_FEATURES = --features=enable-netdebug

all: $(SRC) fonts
	cargo build --release $(RELEASE_FEATURES)
	-mv target/debug/darkfi-app .
	./darkfi-app

dev: $(SRC) fonts
	cargo lbuild $(DEV_FEATURES)
	-mv target/debug/darkfi-app .
	./darkfi-app

android: $(SRC) fonts
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -t apk cargo quad-apk build --release $(RELEASE_FEATURES)
	-mv $(APK) .

ibm-plex-mono-regular.otf:
	wget -c https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/ibm-plex-mono-regular.otf

NotoColorEmoji.ttf:
	wget -c https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/NotoColorEmoji.ttf

fonts: ibm-plex-mono-regular.otf NotoColorEmoji.ttf

apk: darkfi-app.apk
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -t apk cargo quad-apk build --release $(DEV_FEATURES)
	-mv $(APK) .
	adb uninstall darkfi.app
	adb install darkfi-app.apk
	reset
	adb logcat -c
	adb logcat -v color -s darkfi -s SAPP -s libc -s DEBUG -s ActivityManager -s ActivityTaskManager -s WindowManager

# Useful for dev
cli:
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -it apk bash

clean:
	podman run -v $(shell pwd):/root/dw -w /root/dw -t apk rm -fr target/
	rm -f darkfi-app.apk

.PHONY: all android cli clean
