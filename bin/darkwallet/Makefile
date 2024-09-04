default: fonts
	cargo lrun

ibm-plex-mono-light.otf:
	wget https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/ibm-plex-mono-light.otf

NotoColorEmoji.ttf:
	wget https://codeberg.org/darkrenaissance/darkfi/raw/branch/data/NotoColorEmoji.ttf

fonts: ibm-plex-mono-light.otf NotoColorEmoji.ttf

android: fonts
	podman run -v $(shell pwd)/../../:/root/darkfi -w /root/darkfi/bin/darkwallet/ -t apk cargo quad-apk build
	adb uninstall darkfi.darkwallet
	adb install target/android-artifacts/debug/apk/darkwallet.apk
	reset
	adb logcat -c
	adb logcat -s darkfi

clean:
	podman run -v $(shell pwd):/root/dw -w /root/dw -t apk rm -fr target/

