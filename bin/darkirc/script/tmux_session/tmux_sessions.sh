#!/bin/sh
# Start a tmux session of four darkirc nodes, and optionally four weechat clients.
set -e

tmux new-session -s "darkirc" -n "darkirc" -d
tmux send-keys "../../../../darkirc --config seed.toml --skip-dag-sync" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../../darkirc --config darkirc_full_node1.toml --skip-dag-sync" Enter && sleep 1
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
	tmux send-keys "weechat -t -r '/server add darkirc_a 127.0.0.1/22022 -notls;/connect darkirc_a;/set irc.server_default.nicks Alice'" Enter
	tmux split-window -v
	tmux send-keys "weechat -t -r '/server add darkirc_b 127.0.0.1/22023 -notls;/connect darkirc_b;/set irc.server_default.nicks Bob'" Enter
	tmux split-window -h
	tmux send-keys "weechat -t -r '/server add darkirc_c 127.0.0.1/22024 -notls;/connect darkirc_c;/set irc.server_default.nicks Charlie'" Enter
	tmux select-pane -t 0
	tmux split-window -h
	tmux send-keys "weechat -t -r '/server add darkirc_d 127.0.0.1/22025 -notls;/connect darkirc_d;/set irc.server_default.nicks Dave'" Enter
fi

tmux attach
