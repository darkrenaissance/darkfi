#!/bin/sh
bins="$(find bin -maxdepth 1 | tail -n+2 | sed 's,bin/,,' | tr '\n' ' ')"
make BINS="$bins"
