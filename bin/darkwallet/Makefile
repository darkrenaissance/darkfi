default:
	cargo lrun

android:
	docker run -v $(shell pwd):/root/dw -w /root/dw -t apk cargo quad-apk build
	adb uninstall darkfi.darkwallet
	adb install target/android-artifacts/debug/apk/darkwallet.apk
	reset
	adb logcat -c
	adb logcat -s darkfi

clean:
	docker run -v $(shell pwd):/root/dw -w /root/dw -t apk rm -fr target/

