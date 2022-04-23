#!/bin/sh
set -e

find_packages() {
	find bin -type f -name Cargo.toml | while read line; do
		echo "$(basename "$(dirname "$line")")"
	done
}

make BINS="$(find_packages | xargs)"
