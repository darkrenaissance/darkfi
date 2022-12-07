#!/bin/sh
set -e

# Start a tmux session with a lilith node, two consensus nodes, a non-consensus
# node, and a faucet that's able to mint tokens.

if [ "$1" = "-v" ]; then
	verbose="-v"
else
	verbose=""
fi

tmux new-session -d
tmux send-keys "LOG_TARGETS='!MessageSubsystem::notify' ../../../lilith ${verbose} -c lilith_config.toml" Enter
sleep 10
tmux split-window -v
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid0.toml" Enter
sleep 10
tmux select-pane -t 0
tmux split-window -h
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid1.toml" Enter
sleep 10
tmux select-pane -t 1
tmux split-window -h
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid2.toml" Enter
sleep 10
tmux select-pane -t 3
tmux split-window -h
tmux send-keys "LOG_TARGETS='!sled,!net' ../../../faucetd ${verbose} -c faucetd.toml" Enter
tmux attach
