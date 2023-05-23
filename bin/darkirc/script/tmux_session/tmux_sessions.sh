#!/bin/sh
# Start a tmux session of four ircd nodes, and optionally four weechat clients.
set -e

tmux new-session -s "darkirc" -n "darkirc" -d
# tmux send-keys "../../../../darkirc --localnet --config seed.toml" Enter && sleep 1
# tmux split-window -h
tmux send-keys "../../../../darkirc --config ircd_full_node1.toml" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../../darkirc --config ircd_full_node2.toml" Enter
tmux select-pane -t 0
tmux split-window -v
tmux send-keys "../../../../darkirc --config ircd_full_node3.toml" Enter
tmux select-pane -t 2
tmux split-window -v
tmux send-keys "../../../../darkirc --config ircd_full_node4.toml" Enter

if [ -z "$1" ]; then
	tmux new-window -t "darkirc:1" -n "weechat"
	tmux send-keys "weechat -t -r '/server add ircd_a 127.0.0.1/22022;/connect ircd_a;/nick Alice'" Enter
	tmux split-window -v
	tmux send-keys "weechat -t -r '/server add ircd_b 127.0.0.1/22023;/connect ircd_b;/nick Bob'" Enter
	tmux split-window -h
	tmux send-keys "weechat -t -r '/server add ircd_c 127.0.0.1/22024;/connect ircd_c;/nick Charlie'" Enter
	tmux select-pane -t 0
	tmux split-window -h
	tmux send-keys "weechat -t -r '/server add ircd_d 127.0.0.1/22025;/connect ircd_d;/nick Dave'" Enter
fi

tmux attach
