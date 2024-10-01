# Linux

```
make
```

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

This is just devs.

## Debugging Missing Symbols

```
"hb_ft_font_create_referenced"

nm libharfbuzz_rs-5d6b743170eb0207.rlib | grep hb_ | less
```

## Resolve Dependency Issues

```
cargo tree --target aarch64-linux-android --invert openssl-sys
```

