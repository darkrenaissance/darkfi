#!/bin/sh
set -e

# Start a tmux session of three ircd nodes.

tmux new-session -s "ircd" -n "ircd" -d
tmux send-keys "../../../ircd --config ircd_a.toml" Enter
sleep 1
tmux split-window -v
tmux send-keys "../../../ircd --config ircd_b.toml" Enter
sleep 1
tmux select-pane -t 0
tmux split-window -h
tmux send-keys "../../../ircd --config ircd_c.toml" Enter

if [ -z "$1" ]; then
	tmux new-window -t "ircd:1" -n weechat
	tmux send-keys "weechat -t -r '/server add ircd_a 127.0.0.1/6667;/connect ircd_a'" Enter
	sleep 0.5
	tmux split-window -v
	tmux send-keys "weechat -t -r '/server add ircd_b 127.0.0.1/6668;/connect ircd_b'" Enter
	sleep 0.5
	tmux split-window -v
	tmux send-keys "weechat -t -r '/server add ircd_c 127.0.0.1/6669;/connect ircd_c'" Enter
fi

tmux attach
