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
