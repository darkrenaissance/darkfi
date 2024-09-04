# Linux

```
make
```

# Android

Make sure you have podman installed. Then run `make android`.

To debug any issues, you can enter an interactive terminal using:

```
podman run -v $(pwd):/root/dw -it apk bash
```

# Debugging Missing Symbols (note to self)

```
"hb_ft_font_create_referenced"

nm libharfbuzz_rs-5d6b743170eb0207.rlib | grep hb_ | less
```

