# Unix (Linux/Mac)

Install [rustup](rustup.rs) and `cargo install cargo-limit`, then run:

```
make
```

# Windows

Built by cross-compiling on Linux using [cargo-xwin] (MSVC ABI, clang-cl/lld-link).
Also install [rustup](rustup.rs).

Distro packages needed (example uses Void Linux):

```
# xbps-install -S clang lld ninja nasm wget \
    cross-x86_64-w64-mingw32 cross-x86_64-w64-mingw32-crt
```

The mingw packages provide `windres`, which is used to embed the app icon
and the Windows version resource turso needs (`rc.exe` does not exist on
Linux). On Arch the equivalents are `clang lld ninja nasm mingw-w64-gcc`,
on Debian `clang lld ninja-build nasm gcc-mingw-w64-x86-64`, and on Fedora
`clang lld ninja-build nasm mingw64-gcc`.

Then set up the Rust side:

```
$ rustup target add x86_64-pc-windows-msvc
$ rustup component add llvm-tools
$ cargo install --locked cargo-xwin
```

And build:

```
$ make win64-msvc
```

The first run downloads the Windows SDK and MSVC CRT (a couple of GB) and
caches them. Using cargo-xwin implies accepting Microsoft's SDK license.
The resulting `darkfi-app.exe` is placed in `bin/app/`, and can be tested
on Linux using `wine darkfi-app.exe`.

If you get the error "VCRUNTIME140.dll was not found", then
install [Microsoft Visual C++ Redistributable][msvc++].

[cargo-xwin]: https://github.com/rust-cross/cargo-xwin
[msvc++]: https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170#visual-studio-2015-2017-2019-and-2022

# Android

Make sure you have podman installed. Then run:

```
# You only need to build the container once
podman build -t apk .

make android
```

To debug any issues, you can enter an interactive terminal using `make podman-cli`.

To delete everything, run `podman system reset`.

Users who prefer to build locally can follow the commands in the `Dockerfile`.
Note that the `build.rs` hardcodes the SDK/NDK paths so either you follow it
exactly (recommended) or modify `build.rs`.

## GrapheneOS

```
$ adb shell pm list users
    UserInfo{10:Work:30}
$ adb install --user 10 darkfi-app.apk
```

# ADB Over Wifi

Useful for reading the logs without having to be plugged in.
First get your local IP addr using `adb shell ip -f inet a show wlan0`.
Make sure "Wireless debugging" is enabled in Developer options.
Then run:

```
adb tcpip 5555
adb connect IPADDR
```

Copying the APK takes a long time over wifi so best to install
APK via USB, then use this just for debugging the app.

In the Makefile, make sure to put the USB device for `ADB_DEVICES`
so the ADB commands work over USB.

# Useful Dev Commands

This is just for devs. Users ignore this.

## Debugging Missing Symbols

```
"hb_ft_font_create_referenced"

nm libharfbuzz_rs-5d6b743170eb0207.rlib | grep hb_ | less
```

## Resolve Dependency Issues

```
cargo tree --target aarch64-linux-android --invert openssl-sys
```

## Examine the APK

```
apktool d target/android-artifacts/release/apk/darkwallet.apk -o dw-apk
```
