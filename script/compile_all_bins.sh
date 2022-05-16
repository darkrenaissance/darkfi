#!/bin/sh
set -e

# foo|bar|baz
skip_bins='cashierd'

find_packages() {
	find bin -type f -name Cargo.toml | while read line; do
		if echo "$line" | grep -Eq "$skip_bins"; then
			continue
		fi

		echo "$(basename "$(dirname "$line")")"
	done
}

make BINS="$(find_packages | xargs)"
