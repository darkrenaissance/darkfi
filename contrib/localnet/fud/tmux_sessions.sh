#!/bin/sh
set -e

# Start a tmux session with a lilith node and two fud nodes.

# Path to used binaries
LILITH="../../../lilith -c lilith_config.toml"
FUD="../../../fud"

session=fud

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d -s $session -n "lilith"
tmux send-keys -t $session "$LILITH $verbose" Enter
sleep 2
tmux new-window -t $session -n "node0"
tmux send-keys -t $session "$FUD $verbose -c fud_config0.toml" Enter
sleep 2
tmux new-window -t $session -n "node1"
tmux send-keys -t $session "$FUD $verbose -c fud_config1.toml" Enter
tmux attach -t $session
