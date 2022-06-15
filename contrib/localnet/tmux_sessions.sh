#!/bin/sh
set -e

# Start a tmux session with two consensus nodes and a non-consensus node, and
# a faucet that's able to mint tokens.

tmux new-session -d
tmux send-keys "LOG_TARGETS='!sled' ../../darkfid -v -c darkfid0.toml" Enter
sleep 2
tmux split-window -v
tmux send-keys "LOG_TARGETS='!sled' ../../darkfid -v -c darkfid1.toml" Enter
sleep 2
tmux split-window -h
tmux send-keys "LOG_TARGETS='!sled' ../../darkfid -v -c darkfid2.toml" Enter
sleep 2
tmux select-pane -t 0
tmux split-window -h
tmux send-keys "LOG_TARGETS='!sled,!net' ../../faucetd -v -c faucetd.toml" Enter
tmux attach
