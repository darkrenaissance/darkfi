default: fonts
	cargo lrun
	mv target/debug/darkwallet .
	./darkwallet

ibm-plex-mono-regular.otf:
	wget https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/ibm-plex-mono-regular.otf

NotoColorEmoji.ttf:
	wget https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/NotoColorEmoji.ttf

fonts: ibm-plex-mono-regular.otf NotoColorEmoji.ttf

android: fonts
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/darkwallet/ -t apk cargo quad-apk build --release
	adb uninstall darkfi.darkwallet
	adb install target/android-artifacts/release/apk/darkwallet.apk
	reset
	adb logcat -c
	adb logcat -s darkfi

# Useful for dev
cli:
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/darkwallet/ -it apk bash

clean:
	podman run -v $(shell pwd):/root/dw -w /root/dw -t apk rm -fr target/

