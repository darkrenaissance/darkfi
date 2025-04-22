.POSIX:

# Cargo binary
CARGO = cargo

RELEASE_APK = target/android-artifacts/release/apk/darkfi-app.apk
DEBUG_APK = target/android-artifacts/debug/apk/darkfi-app.apk

SRC = \
	build.rs \
	Cargo.lock \
	Cargo.toml \
	Dockerfile \
	$(shell find assets -type f) \
	$(shell find src -type f)

RELEASE_FEATURES = --features=enable-plugins
DEBUG_FEATURES = --features=enable-filelog,enable-plugins

#DEV_FEATURES = --features=enable-filelog,enable-netdebug,emulate-android
#DEV_FEATURES = --features=enable-filelog,enable-netdebug,enable-plugins
DEV_FEATURES = --features=enable-netdebug

default: build-release
	./darkfi-app

android: android-release

# Platform release builds

macos-release: build-release
	-mv darkfi-app darkfi-app.macos
macos-debug: build-debug
	-mv darkfi-app darkfi-app_debug.macos
linux-release: build-release
	-mv darkfi-app darkfi-app.linux
linux-debug: build-debug
	-mv darkfi-app darkfi-app_debug.linux
win-release: $(SRC) fonts
	$(CARGO) build --release $(RELEASE_FEATURES)
	-mv target/release/darkfi-app.exe .
win-debug: $(SRC) fonts
	$(CARGO) build $(DEBUG_FEATURES)
	-mv target/debug/darkfi-app.exe .
android-release: $(SRC) fonts
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -t apk cargo quad-apk build --release $(RELEASE_FEATURES)
	-mv $(RELEASE_APK) darkfi-app.apk
android-debug: $(SRC) fonts
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -t apk cargo quad-apk build $(DEBUG_FEATURES)
	-mv $(DEBUG_APK) darkfi-app_debug.apk

build-release: $(SRC) fonts
	$(CARGO) build --release $(RELEASE_FEATURES)
	-mv target/release/darkfi-app .
build-debug: $(SRC) fonts
	$(CARGO) build $(DEBUG_FEATURES)
	-mv target/debug/darkfi-app .

# Download font data

fonts: ibm-plex-mono-regular.otf NotoColorEmoji.ttf

ibm-plex-mono-regular.otf:
	wget -c https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/ibm-plex-mono-regular.otf

NotoColorEmoji.ttf:
	wget -c https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/NotoColorEmoji.ttf

# Developer targets

dev: $(SRC) fonts
	$(CARGO) lbuild $(DEV_FEATURES)
	-mv target/debug/darkfi-app .
	./darkfi-app

apk: $(SRC) fonts
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -t apk cargo quad-apk build $(DEV_FEATURES)
	-mv $(DEBUG_APK) .
	-adb uninstall darkfi.darkfi_app
	adb install -r darkfi-app.apk
	reset
	adb logcat -c
	adb logcat -v color -s darkfi -s SAPP -s libc -s DEBUG -s ActivityManager -s ActivityTaskManager -s WindowManager -s AndroidRuntime -s rkfi.darkfi_app

# Useful for dev
cli:
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -it apk bash

fmt:
	$(CARGO) +nightly fmt

clean:
	podman run -v $(shell pwd):/root/dw -w /root/dw -t apk rm -fr target/
	rm -f darkfi-app.apk

.PHONY: all android cli clean
