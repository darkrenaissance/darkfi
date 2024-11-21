#!/bin/sh
set -e

# Start a tmux session of four dchatd nodes.

# Path to `dchat` binary
DCHATD="../../../target/release/dchatd"

session=dchatd

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

mkdir -p logs

tmux new-session -d -s $session -n "seed"
tmux send-keys -t $session "$DCHATD $verbose -c seed.toml 2>&1 | tee logs/seed.log" Enter
sleep 1
tmux new-window -t $session -n "node1"
tmux send-keys -t $session "$DCHATD $verbose -c dchat1.toml 2>&1 | tee logs/dchat1.log" Enter
sleep 1
tmux new-window -t $session -n "node2"
tmux send-keys -t $session "$DCHATD $verbose -c dchat2.toml 2>&1 | tee logs/dchat2.log" Enter
sleep 1
tmux new-window -t $session -n "node3"
tmux send-keys -t $session "$DCHATD $verbose -c dchat3.toml 2>&1 | tee logs/dchat3.log" Enter
sleep 1
tmux new-window -t $session -n "node4"
tmux send-keys -t $session "$DCHATD $verbose -c dchat4.toml 2>&1 | tee logs/dchat4.log" Enter
tmux attach -t $session
