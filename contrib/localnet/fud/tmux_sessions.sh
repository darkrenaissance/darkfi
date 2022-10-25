#!/bin/sh
set -e

# Start a tmux session with a lilith node and two fud nodes.

if [ "$1" = "-v" ]; then
	verbose="-v"
else
	verbose=""
fi

tmux new-session -d
tmux send-keys "../../../lilith ${verbose} -c lilith_config.toml" Enter
sleep 2
tmux split-window -v
tmux send-keys "../../../fud ${verbose} -c fud_config0.toml" Enter
sleep 2
tmux select-pane -t 1
tmux split-window -h
tmux send-keys "../../../fud ${verbose} -c fud_config1.toml" Enter
tmux attach
