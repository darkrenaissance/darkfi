#!/bin/sh
set -e

find_packages() {
	find bin -type f -name Cargo.toml | while read line; do
		if echo "$line" | grep -Eq 'cashierd|darkfid|gatewayd'; then
			continue
		fi

		echo "$(basename "$(dirname "$line")")"
	done
}

make BINS="$(find_packages | xargs)"
