#!/bin/sh
set -e

# Start a tmux session with a consensus nodes and a faucet that's able to mint tokens.

if [ "$1" = "-v" ]; then
	verbose="-v"
	shift
else
	verbose=""
fi

if [ "$1" = "now" ]; then
	now="$(date +%s)"

	sed \
		-e 's/pub const SLOT_TIME: .*/pub const SLOT_TIME: u64 = 20;/' \
		-e 's/pub const FINAL_SYNC_DUR: .*/pub const FINAL_SYNC_DUR: u64 = 15;/' \
		-e "s/pub static ref TESTNET_GENESIS_TIMESTAMP: .*/pub static ref TESTNET_GENESIS_TIMESTAMP: Timestamp = Timestamp($now);/" \
		-e "s/pub static ref TESTNET_BOOTSTRAP_TIMESTAMP: .*/pub static ref TESTNET_BOOTSTRAP_TIMESTAMP: Timestamp = Timestamp($now);/" \
		-i ../../../src/consensus/constants.rs

	exit
fi

tmux new-session -d
tmux send-keys "LOG_TARGETS='!sled,!net' ../../../darkfid ${verbose} -c darkfid0.toml" Enter
sleep 10
tmux split-window -v
tmux send-keys "LOG_TARGETS='!sled,!net' ../../../faucetd ${verbose} -c faucetd.toml" Enter
tmux attach
