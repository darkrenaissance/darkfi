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
# Start a tmux session of five ircd nodes, and optionally 5 weechat clients.
set -e

tmux new-session -s "ircd" -n "ircd" -d
tmux send-keys "../../../ircd --config node1.toml" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../ircd --config node2.toml" Enter && sleep 1

if [ -z "$1" ]; then
	tmux new-window -t "ircd:1" -n "weechat"
	tmux send-keys "weechat -t -r '/server add node1 127.0.0.1/22022;/connect node1'" Enter
	tmux split-window -v
	tmux send-keys "weechat -t -r '/server add node2 127.0.0.1/22023;/connect node2'" Enter
fi

tmux attach
