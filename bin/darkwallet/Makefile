default:
	cargo lrun

android:
	docker run -v $(shell pwd):/root/dw -v /tmp/miniquad:/tmp/miniquad -w /root/dw -t apk cargo quad-apk build
	adb uninstall rust.darkwallet
	adb install target/android-artifacts/debug/apk/darkwallet.apk

clean:
	docker run -v $(shell pwd):/root/dw -w /root/dw -t apk rm -fr target/

