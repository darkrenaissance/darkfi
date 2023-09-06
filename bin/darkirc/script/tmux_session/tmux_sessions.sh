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
# Start a tmux session of four darkirc nodes, and optionally four weechat clients.
set -e

tmux new-session -s "darkirc" -n "darkirc" -d
tmux send-keys "../../../../darkirc --config seed.toml" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../../darkirc --config darkirc_full_node1.toml" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../../darkirc --config darkirc_full_node2.toml" Enter
tmux select-pane -t 0
tmux split-window -v
tmux send-keys "../../../../darkirc --config darkirc_full_node3.toml" Enter
tmux select-pane -t 2
tmux split-window -v
tmux send-keys "../../../../darkirc --config darkirc_full_node4.toml" Enter

if [ -z "$1" ]; then
	tmux new-window -t "darkirc:1" -n "weechat"
	tmux send-keys "weechat -t -r '/server add darkirc_a 127.0.0.1/22022 -notls;/connect darkirc_a;/nick Alice'" Enter
	tmux split-window -v
	tmux send-keys "weechat -t -r '/server add darkirc_b 127.0.0.1/22023 -notls;/connect darkirc_b;/nick Bob'" Enter
	tmux split-window -h
	tmux send-keys "weechat -t -r '/server add darkirc_c 127.0.0.1/22024 -notls;/connect darkirc_c;/nick Charlie'" Enter
	tmux select-pane -t 0
	tmux split-window -h
	tmux send-keys "weechat -t -r '/server add darkirc_d 127.0.0.1/22025 -notls;/connect darkirc_d;/nick Dave'" Enter
fi

tmux attach
