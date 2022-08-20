#!/bin/sh
set -e

# Start a tmux session with a lilith node and two fud nodes.

tmux new-session -d
tmux new-session -d
tmux send-keys "../../../../lilith -c lilith_config.toml" Enter
sleep 2
tmux split-window -v
tmux send-keys "../fud/target/debug/fud -c fud_config0.toml" Enter
sleep 2
tmux select-pane -t 1
tmux split-window -h
tmux send-keys "../fud/target/debug/fud -c fud_config1.toml" Enter
tmux attach
