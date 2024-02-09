/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

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
