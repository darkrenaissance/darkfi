#!/bin/sh
set -e

# Start a tmux session with a minerd daemon and a darkfid node

# Path to used binaries
MINERD="../../../minerd -c minerd.toml"
DARKFID="LOG_TARGETS='!net,!runtime,!sled' ../../../darkfid -c darkfid.toml"

session=darkfid-single-node

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d -s $session -n $session
tmux send-keys -t $session "$MINERD $verbose" Enter
sleep 1
tmux split-window -t $session -v -l 90%
tmux send-keys -t $session "$DARKFID $verbose" Enter
tmux attach -t $session
