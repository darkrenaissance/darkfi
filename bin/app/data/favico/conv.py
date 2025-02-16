#!/usr/bin/env python

from PIL import Image

def make(name, size):
    image = Image.open(f"favico{size}.png")
    image = image.convert('RGBA')

    pixels = list(image.getdata())
    print(f"pub const {name}: [u8; {size} * {size} * 4] = [")
    for pixel in pixels:
        pixel = [str(p) for p in pixel]
        print(", ".join(pixel) + ",")
    print("];")

make("SMALL", 16)
make("MEDIUM", 32)
make("BIG", 64)

