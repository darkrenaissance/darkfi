#!/bin/sh
set -e

# RandomX activation height:
# Mainnet: 1978433
# Testnet: 1308737

MONERO_VERSION="0.18"

MONEROD_BIN="./monero/build/Linux/release-v${MONERO_VERSION}/release/bin/monerod"

if ! [ -f "$MONEROD_BIN" ]; then
	echo "Could not find monerod, perhaps run ./build-monerod.sh first?"
	exit 1
fi

"${MONEROD_BIN}" --regtest --fixed-difficulty 1 \
	--offline --log-level 0 --keep-fakechain
