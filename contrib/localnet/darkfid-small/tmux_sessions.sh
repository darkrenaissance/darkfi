#!/bin/sh
set -e

# Start a tmux session with two mining and a non-mining darkfid nodes.
# Additionally, start two minerd daemons.

# Path to used binaries
MINERD="../../../minerd"
DARKFID="LOG_TARGETS='!runtime,!sled' ../../../darkfid"

session=darkfid-small

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d -s $session -n "node0"
tmux send-keys -t $session "$MINERD -c minerd0.toml $verbose" Enter
sleep 1
tmux split-window -t $session -v -l 90%
tmux send-keys -t $session "$DARKFID -c darkfid0.toml $verbose" Enter
sleep 2
tmux new-window -t $session -n "node1"
tmux send-keys -t $session "$MINERD -c minerd1.toml $verbose" Enter
sleep 1
tmux split-window -t $session -v -l 90%
tmux send-keys -t $session "$DARKFID -c darkfid1.toml $verbose" Enter
sleep 2
tmux new-window -t $session -n "node2"
tmux send-keys -t $session "$DARKFID -c darkfid2.toml $verbose" Enter
tmux attach -t $session
