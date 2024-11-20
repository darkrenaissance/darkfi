#!/bin/sh
# Start a tmux session of four taud nodes, and four tau clients.
set -e

tmux new-session -s "taud" -n "taud" -d
tmux send-keys "../../../taud --config seed.toml --skip-dag-sync" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../taud --config taud_full_node1.toml --skip-dag-sync" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../taud --config taud_full_node2.toml" Enter
tmux select-pane -t 0
tmux split-window -v
tmux send-keys "../../../taud --config taud_full_node3.toml" Enter
tmux select-pane -t 2
tmux split-window -v
tmux send-keys "../../../taud --config taud_full_node4.toml" Enter

if [ -z "$1" ]; then
	tmux new-window -t "taud:1" -n "tau"
	tmux send-keys "tau -e 127.0.0.1:23341" Enter
	tmux split-window -v
	tmux send-keys "tau -e 127.0.0.1:23342" Enter
	tmux split-window -h
	tmux send-keys "tau -e 127.0.0.1:23343" Enter
	tmux select-pane -t 0
	tmux split-window -h
	tmux send-keys "tau -e 127.0.0.1:23344" Enter
fi

tmux attach
