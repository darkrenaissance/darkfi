# Unix (Linux/Mac)

Install [rustup](rustup.rs) and `cargo install cargo-limit`, then run:

```
make
```

# Windows

Install [rustup](rustup.rs) and follow the instructions.

If you get the error "VCRUNTIME140.dll was not found", then
install [Microsoft Visual C++ Redistributable][msvc++].

[msvc++]: https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist?view=msvc-170#visual-studio-2015-2017-2019-and-2022

# Android

Make sure you have podman installed. Then run:

```
# You only need to build the container once
podman build -t apk .

make android
```

To debug any issues, you can enter an interactive terminal using `make cli`.

To delete everything, run `podman system reset`.

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
