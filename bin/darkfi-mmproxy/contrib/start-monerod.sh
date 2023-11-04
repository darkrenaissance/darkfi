#!/bin/sh
set -e

MONERO_VERSION="0.18"

MONEROD_BIN="./monero/build/Linux/release-v${MONERO_VERSION}/release/bin/monerod"

if ! [ -f "$MONEROD_BIN" ]; then
	echo "Could not find monerod, perhaps run ./build-monerod.sh first?"
	exit 1
fi

"${MONEROD_BIN}" --testnet --fixed-difficulty 2 \
	--offline --hide-my-port --log-level 4
