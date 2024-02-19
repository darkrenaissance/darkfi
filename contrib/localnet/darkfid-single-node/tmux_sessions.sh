#!/bin/sh
set -e

# Start a tmux session with a minerd daemon and a darkfid node

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d
tmux send-keys "../../../minerd ${verbose} -c minerd.toml" Enter
sleep 1
tmux split-window -v
tmux send-keys "LOG_TARGETS='!sled,!net' ../../../darkfid ${verbose} -c darkfid.toml" Enter
tmux attach
