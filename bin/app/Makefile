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
DEV_FEATURES = --features=enable-plugins

default: build-release
	./darkfi-app

android: android-release

# Platform release builds

ios-release: $(SRC) fonts forest_720x1280 forest_1920x1080
	# Create the app bundle structure
	rm -rf DarkFi.app
	mkdir -p DarkFi.app
	# Build for iOS device (aarch64)
	SDKROOT=$$(xcrun --sdk iphoneos --show-sdk-path 2>/dev/null); \
	if [ -z "$$SDKROOT" ]; then \
		SDKROOT=$$(xcodebuild -version -sdk iphoneos Path 2>/dev/null); \
	fi; \
	if [ -z "$$SDKROOT" ]; then \
		echo "ERROR: Could not find iphoneos SDK."; \
		echo "Current xcode-select path: $$(xcode-select -p)"; \
		echo "Ensure Xcode is installed and selected: sudo xcode-select -s /Applications/Xcode.app"; \
		exit 1; \
	fi; \
	export SDKROOT=$$SDKROOT; \
	export IPHONEOS_DEPLOYMENT_TARGET=14.0; \
	cargo build --target aarch64-apple-ios --release $(RELEASE_FEATURES)
	# Copy the binary
	cp target/aarch64-apple-ios/release/darkfi-app DarkFi.app/
	# Copy resources
	cp ios-Info.plist DarkFi.app/Info.plist
	cp -r assets/lang DarkFi.app/
	cp -r assets/forest_720x1280 DarkFi.app/
	cp -r assets/forest_1920x1080 DarkFi.app/
	cp assets/*.png DarkFi.app/ || true
	cp embedded.mobileprovision DarkFi.app/ || echo "Warning: No provisioning profile found. Device builds will fail to launch."
	cp ibm-plex-mono-regular.otf DarkFi.app/
	cp NotoColorEmoji.ttf DarkFi.app/
	# Sign the app bundle with entitlements
	codesign --force --sign - --entitlements DarkFi.entitlements --timestamp=none DarkFi.app
	# Create a dummy PkgInfo (optional but good practice)
	echo "APPL????" > DarkFi.app/PkgInfo
	@echo "DarkFi.app built for device (aarch64)."

ios-sim: $(SRC) fonts forest_720x1280 forest_1920x1080
	# Create the app bundle structure
	rm -rf DarkFi.app
	mkdir -p DarkFi.app
	# Build for iOS simulator (Detect Arch)
	SDKROOT=$$(xcrun --sdk iphonesimulator --show-sdk-path 2>/dev/null); \
	if [ -z "$$SDKROOT" ]; then \
		SDKROOT=$$(xcodebuild -version -sdk iphonesimulator Path 2>/dev/null); \
	fi; \
	if [ -z "$$SDKROOT" ]; then \
		echo "ERROR: Could not find iphonesimulator SDK."; \
		echo "Current xcode-select path: $$(xcode-select -p)"; \
		echo "Ensure Xcode is installed and selected: sudo xcode-select -s /Applications/Xcode.app"; \
		exit 1; \
	fi; \
	export SDKROOT=$$SDKROOT; \
	export IPHONEOS_DEPLOYMENT_TARGET=14.0; \
	ARCH=$$(uname -m); \
	if [ "$$ARCH" = "arm64" ]; then \
		echo "Detected Apple Silicon (arm64). Building for aarch64-apple-ios-sim..."; \
		cargo build --target aarch64-apple-ios-sim --release $(RELEASE_FEATURES); \
		cp target/aarch64-apple-ios-sim/release/darkfi-app DarkFi.app/; \
	else \
		echo "Detected Intel (x86_64). Building for x86_64-apple-ios..."; \
		cargo build --target x86_64-apple-ios --release $(RELEASE_FEATURES); \
		cp target/x86_64-apple-ios/release/darkfi-app DarkFi.app/; \
	fi
	# Copy resources
	cp ios-Info.plist DarkFi.app/Info.plist
	cp -r assets/lang DarkFi.app/
	cp -r assets/forest_720x1280 DarkFi.app/
	cp -r assets/forest_1920x1080 DarkFi.app/
	cp assets/*.png DarkFi.app/ || true
	cp ibm-plex-mono-regular.otf DarkFi.app/
	cp NotoColorEmoji.ttf DarkFi.app/
	# Sign the app bundle with entitlements
	codesign --force --sign - --entitlements DarkFi.entitlements --timestamp=none DarkFi.app
	echo "APPL????" > DarkFi.app/PkgInfo
	@echo "DarkFi.app built for simulator."

ios-xcode:
	rm -rf DarkFi.xcodeproj
	mkdir -p DarkFi.xcodeproj
	# Generate unique IDs for the project file (24 chars)
	sed -e "s/CONFIG_LIST_ID/$$(uuidgen | tr -d - | tr '[:lower:]' '[:upper:]' | cut -c 1-24)/g" \
	    -e "s/MAIN_GROUP_ID/$$(uuidgen | tr -d - | tr '[:lower:]' '[:upper:]' | cut -c 1-24)/g" \
	    -e "s/LEGACY_TARGET_ID/$$(uuidgen | tr -d - | tr '[:lower:]' '[:upper:]' | cut -c 1-24)/g" \
	    -e "s/PROJECT_ID/$$(uuidgen | tr -d - | tr '[:lower:]' '[:upper:]' | cut -c 1-24)/g" \
	    -e "s/PROJECT_CONFIG_LIST_ID/$$(uuidgen | tr -d - | tr '[:lower:]' '[:upper:]' | cut -c 1-24)/g" \
	    -e "s/DEBUG_CONFIG_ID/$$(uuidgen | tr -d - | tr '[:lower:]' '[:upper:]' | cut -c 1-24)/g" \
	    -e "s/RELEASE_CONFIG_ID/$$(uuidgen | tr -d - | tr '[:lower:]' '[:upper:]' | cut -c 1-24)/g" \
	    project.pbxproj.template > DarkFi.xcodeproj/project.pbxproj
	@echo "Generated DarkFi.xcodeproj. Open it in Xcode and ensure the 'External Build Tool' configuration points to 'make' and target 'ios-release' in this directory."

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
android-release: $(SRC) fonts forest_720x1280
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -t apk cargo quad-apk build --release $(RELEASE_FEATURES)
	-mv $(RELEASE_APK) darkfi-app.apk
android-debug: $(SRC) fonts forest_720x1280
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -t apk cargo quad-apk build $(DEBUG_FEATURES)
	-mv $(DEBUG_APK) darkfi-app_debug.apk

build-release: $(SRC) fonts forest_1920x1080
	$(CARGO) build --release $(RELEASE_FEATURES)
	-mv target/release/darkfi-app .
build-debug: $(SRC) fonts forest_1920x1080
	$(CARGO) build $(DEBUG_FEATURES)
	-mv target/debug/darkfi-app .

# Download font data

fonts: ibm-plex-mono-regular.otf NotoColorEmoji.ttf

ibm-plex-mono-regular.otf:
	wget -c https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/ibm-plex-mono-regular.otf

NotoColorEmoji.ttf:
	wget -c https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/NotoColorEmoji.ttf

forest_1920x1080.zip:
	wget -c https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/forest_1920x1080.zip
assets/forest_1920x1080/000.qoi:
	cd assets && unzip ../forest_1920x1080.zip
forest_1920x1080: forest_1920x1080.zip assets/forest_1920x1080/000.qoi
	# rm -fr assets/forest_729x1280/

forest_720x1280.zip:
	wget -c https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/forest_720x1280.zip
assets/forest_720x1280/000.qoi:
	cd assets && unzip ../forest_720x1280.zip
forest_720x1280: forest_720x1280.zip assets/forest_720x1280/000.qoi
	# rm -fr assets/forest_1920x1080/

# Developer targets

dev: $(SRC) fonts forest_1920x1080
	$(CARGO) lbuild $(DEV_FEATURES)
	-mv target/debug/darkfi-app .
	./darkfi-app

apk: $(SRC) fonts forest_720x1280
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -t apk cargo quad-apk build $(DEV_FEATURES)
	$(MAKE) install-apk

install-apk:
	-mv $(DEBUG_APK) .
	-adb $(ADB_DEVICE) uninstall darkfi.darkfi_app
	adb $(ADB_DEVICE) install -r darkfi-app.apk
	reset
	adb $(ADB_DEVICE) logcat -c
	adb $(ADB_DEVICE) shell monkey -p darkfi.darkfi_app -c android.intent.category.LAUNCHER 1
	adb $(ADB_DEVICE) logcat -v color -s darkfi -s SAPP -s libc -s DEBUG -s ActivityManager -s ActivityTaskManager -s WindowManager -s AndroidRuntime -s rkfi.darkfi_app | tee output.log

# Useful for dev
cli:
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/app/ -it apk bash

fmt:
	$(CARGO) +nightly fmt

clean:
	podman run -v $(shell pwd):/root/dw -w /root/dw -t apk rm -fr target/
	rm -fr target/
	rm -f darkfi-app.apk
	rm -rf DarkFi.app
	rm -rf DarkFi.xcodeproj

.PHONY: all android cli clean
