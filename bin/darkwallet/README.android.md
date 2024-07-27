```
podman system service
podman build -t apk .
```

```
# This command gives an interactive terminal (useful for debugging):
#podman run -v $(pwd):/root/dw -it apk bash

podman run -v $(pwd):/root/dw -w /root/dw -t apk cargo quad-apk build
```

Enable USB debugging in developer options and run the following commands:

```
adb install -r target/android-artifacts/debug/apk/darkwallet.apk
# Clear the log
adb logcat -c
adb logcat -s darkfi
```

# Debugging Missing Symbols (note to self)

```
"hb_ft_font_create_referenced"

nm libharfbuzz_rs-5d6b743170eb0207.rlib | grep hb_ | less
```

