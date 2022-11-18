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
