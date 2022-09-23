#!/bin/sh
# Start a tmux session of five ircd nodes, and optionally 5 weechat clients.
set -e

tmux new-session -s "ircd" -n "ircd" -d
tmux send-keys "../../../ircd --config ircd_a.toml" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../ircd --config ircd_b.toml" Enter && sleep 1
tmux select-pane -t 0
tmux split-window -v
tmux send-keys "../../../ircd --config ircd_c.toml" Enter
tmux select-pane -t 2
tmux split-window -v
tmux send-keys "../../../ircd --config ircd_d.toml" Enter
tmux split-window -h
tmux send-keys "../../../ircd --config ircd_e.toml" Enter

if [ -z "$1" ]; then
	tmux new-window -t "ircd:1" -n "weechat"
	tmux send-keys "weechat -t -r '/server add ircd_a 127.0.0.1/25570;/connect ircd_a'" Enter
	tmux split-window -v
	tmux send-keys "weechat -t -r '/server add ircd_b 127.0.0.1/25571;/connect ircd_b'" Enter
	tmux split-window -v
	tmux send-keys "weechat -t -r '/server add ircd_c 127.0.0.1/25572;/connect ircd_c'" Enter
	tmux select-pane -t 0
	tmux split-window -h
	tmux send-keys "weechat -t -r '/server add ircd_d 127.0.0.1/25573;/connect ircd_d'" Enter
	tmux select-pane -t 2
	tmux split-window -h
	tmux send-keys "weechat -t -r '/server add ircd_e 127.0.0.1/25574;/connect ircd_e'" Enter
fi

tmux attach
