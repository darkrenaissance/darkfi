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
# Start a tmux session of 4 genev daemons, and 4 genev clis.
set -e

tmux new-session -s "genevd" -n "genevd" -d
tmux send-keys "../../../genevd --localnet --config genevd_seed.toml" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../genevd --localnet --config genevd_a.toml" Enter
tmux split-window -h
tmux send-keys "../../../genevd --localnet --config genevd_b.toml" Enter
tmux select-pane -t 0
tmux split-window -v
tmux send-keys "../../../genevd --localnet --config genevd_c.toml" Enter
tmux select-pane -t 2
tmux split-window -v
tmux send-keys "../../../genevd --localnet --config genevd_d.toml" Enter


tmux new-window -t "genevd:1" -n "genev"
sleep 1
tmux send-keys "../../../genev -e tcp://127.0.0.1:28870 add alolymous \"pay bills\" \"gonna pay some bills in the morning\" " Enter
tmux split-window -v
sleep 1
tmux send-keys "../../../genev -e tcp://127.0.0.1:28871 list" Enter
tmux split-window -h
tmux send-keys "../../../genev -e tcp://127.0.0.1:28872 list" Enter
tmux select-pane -t 0
tmux split-window -h
tmux send-keys "../../../genev -e tcp://127.0.0.1:28873 list" Enter


tmux attach
