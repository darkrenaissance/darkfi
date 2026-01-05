.POSIX:

# Cargo binary
CARGO = cargo

RELEASE_APK = target/android-artifacts/release/apk/darkfi-app.apk
DEBUG_APK = target/android-artifacts/debug/apk/darkfi-app.apk

#ADB_DEVICE = -s DEVICE_ID
ADB_DEVICE =

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
DEV_FEATURES = --features=schema-app

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
android-release: $(SRC) fonts assets/forest_720x1280.mp4
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -t apk cargo quad-apk build --release $(RELEASE_FEATURES)
	-mv $(RELEASE_APK) darkfi-app.apk
android-debug: $(SRC) fonts assets/forest_720x1280.mp4
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -t apk cargo quad-apk build $(DEBUG_FEATURES)
	-mv $(DEBUG_APK) darkfi-app_debug.apk

build-release: $(SRC) fonts assets/forest_1920x1080.ivf
	$(CARGO) build --release $(RELEASE_FEATURES)
	-mv target/release/darkfi-app .
build-debug: $(SRC) fonts assets/forest_1920x1080.ivf
	$(CARGO) build $(DEBUG_FEATURES)
	-mv target/debug/darkfi-app .

# Download font data

fonts: ibm-plex-mono-regular.otf NotoColorEmoji.ttf

ibm-plex-mono-regular.otf:
	wget -c https://dark.fi/assets/ibm-plex-mono-regular.otf

NotoColorEmoji.ttf:
	wget -c https://dark.fi/assets/NotoColorEmoji.ttf

# App data

assets/forest_1920x1080.ivf:
	rm -f assets/forest_720x1280.mp4
	wget -P assets/ -c https://dark.fi/assets/forest_1920x1080.ivf

assets/forest_720x1280.mp4:
	rm -f assets/forest_1920x1080.ivf
	wget -P assets/ -c https://dark.fi/assets/forest_720x1280.mp4

# Developer targets

dev: $(SRC) fonts assets/forest_1920x1080.ivf
	$(CARGO) lbuild --no-default-features $(DEV_FEATURES)
	-mv target/debug/darkfi-app .
	./darkfi-app

# Users should use the android-release and android-debug targets instead.
apk: $(SRC) fonts assets/forest_720x1280.mp4
	cargo quad-apk build --no-default-features $(DEV_FEATURES)
	$(MAKE) install-apk

install-apk:
	-mv $(DEBUG_APK) .
	-adb $(ADB_DEVICE) uninstall darkfi.darkfi_app
	adb $(ADB_DEVICE) install -r darkfi-app.apk
	$(MAKE) log-apk

log-apk:
	reset
	adb $(ADB_DEVICE) logcat -c
	adb $(ADB_DEVICE) shell monkey -p darkfi.darkfi_app -c android.intent.category.LAUNCHER 1
	adb $(ADB_DEVICE) logcat -v color -s darkfi -s SAPP -s libc -s DEBUG -s ActivityManager -s ActivityTaskManager -s WindowManager -s AndroidRuntime -s rkfi.darkfi_app | tee output.log

# Useful for dev
podman-cli:
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -it apk bash

fmt:
	$(CARGO) +nightly fmt

clean:
	podman run -v $(shell pwd):/root/dw -w /root/dw -t apk rm -fr target/
	rm -f darkfi-app.apk

.PHONY: all android cli clean
