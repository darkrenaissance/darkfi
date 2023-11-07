#!/bin/sh
set -e

MONERO_VERSION="0.18"

if ! [ -d "monero" ]; then
	git clone "https://github.com/monero-project/monero" \
		-b "release-v${MONERO_VERSION}" monero --recursive
fi

make -C monero release
