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

# Start a tmux session with two consensus and a non-consensus node.

if [ "$1" = "-v" ]; then
	verbose="-v"
else
	verbose=""
fi

tmux new-session -d
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid0.toml" Enter
tmux split-window -v
sleep 2
tmux select-pane -t 0
tmux split-window -h
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid1.toml" Enter
sleep 2
tmux select-pane -t 2
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid2.toml" Enter 
tmux attach
