#!/bin/sh
set -e

# Start a tmux session with five minerd daemons and five darkfid nodes

# Path to used binaries
MINERD="../../../minerd"
DARKFID="LOG_TARGETS='!runtime,!sled' ../../../darkfid"

session=darkfid-five-nodes

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
tmux send-keys -t $session "$MINERD -c minerd2.toml $verbose" Enter
sleep 1
tmux split-window -t $session -v -l 90%
tmux send-keys -t $session "$DARKFID -c darkfid2.toml $verbose" Enter
sleep 2
tmux new-window -t $session -n "node3"
tmux send-keys -t $session "$MINERD -c minerd3.toml $verbose" Enter
sleep 1
tmux split-window -t $session -v -l 90%
tmux send-keys -t $session "$DARKFID -c darkfid3.toml $verbose" Enter
sleep 2
tmux new-window -t $session -n "node4"
tmux send-keys -t $session "$MINERD -c minerd4.toml $verbose" Enter
sleep 1
tmux split-window -t $session -v -l 90%
tmux send-keys -t $session "$DARKFID -c darkfid4.toml $verbose" Enter
tmux attach -t $session
