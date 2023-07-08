#!/bin/sh
set -e

CARGO="${CARGO:-cargo +nightly}"

toplevel="$(git rev-parse --show-toplevel)"
dirs="$(grep '"bin/' "$toplevel/Cargo.toml" | grep -v '#' | tr -d '", \t')"

bins=""
for i in $dirs; do
	bins="$bins $(grep '^name = ' "$i/Cargo.toml" | cut -d' ' -f3 | tr -d '"')"
done

make CARGO="$CARGO" BINS="$bins"
