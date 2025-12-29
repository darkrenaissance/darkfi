#!/usr/bin/env python3

# cp book/book.js /tmp/
# python remove_chapter_nav_js.py
# diff book/book.js /tmp/book.js

from glob import glob

for file in glob("book/book-*.js"):
    with open(file) as f:
        lines = f.read()

    lines = lines.split("\n")

    pre = []
    while True:
        line = lines[0]

        if "chapterNavigation()" in line:
            break

        pre.append(lines.pop(0))

    # chapterNavigation() {
    lines.pop(0)

    i = 1
    while True:
        line = lines.pop(0)
        i += line.count("{") - line.count("}")
        assert i >= 0
        if i == 0:
            break

    src = "\n".join(pre + lines)
    #print(src)

    with open(file, "w") as f:
        f.write(src)
