#!/bin/sh
set -e

# Start a tmux session with 5 consensus nodes.

if [ "$1" = "-v" ]; then
	verbose="-v"
else
	verbose=""
fi

tmux new-session -d
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid0.toml" Enter
sleep 2
tmux split-window -h
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid1.toml" Enter
sleep 2
tmux split-window -h
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid2.toml" Enter
sleep 2
tmux split-window -h
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid3.toml" Enter
sleep 2
tmux split-window -h
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid4.toml" Enter
tmux select-layout even-horizontal
tmux attach
