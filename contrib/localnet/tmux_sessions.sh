#!/bin/sh
set -e

# Start a tmux session with two consensus nodes and a non-consensus node, and
# a faucet that's able to mint tokens.

tmux new-session -d "LOG_TARGETS='!sled' ../../darkfid2 -v -c darkfid0.toml"
sleep 2
tmux split-window -v "LOG_TARGETS='!sled' ../../darkfid2 -v -c darkfid1.toml"
sleep 2
tmux split-window -h "LOG_TARGETS='!sled' ../../darkfid2 -v -c darkfid2.toml"
sleep 2
tmux select-pane -t 0
tmux split-window -h "LOG_TARGETS='!sled' ../../faucetd -v -c faucetd.toml"
tmux attach
