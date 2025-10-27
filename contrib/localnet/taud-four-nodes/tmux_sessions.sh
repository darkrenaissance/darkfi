#!/bin/sh
set -e

# Start a tmux session of four taud nodes, and four tau clients.

# Path to used binaries
TAUD="../../../taud"
TAU_CLI="../../../bin/tau/tau-python"
TAU="python $TAU_CLI/tau"

# Source tau-cli python venv
. $TAU_CLI/venv/bin/activate

session=taud-local

tmux new-session -d -s $session -n "seed"
tmux send-keys -t $session "$TAUD --config seed.toml --skip-dag-sync" Enter
sleep 1
tmux new-window -t $session -n "node1"
tmux send-keys -t $session "$TAUD --config taud_full_node1.toml --skip-dag-sync" Enter
if [ "$1" ]; then
	tmux split-window -t $session -v
	tmux send-keys -t $session "$TAU -e 127.0.0.1:23341" Enter
fi
sleep 1
tmux new-window -t $session -n "node2"
tmux send-keys -t $session "$TAUD --config taud_full_node2.toml" Enter
if [ "$1" ]; then
	tmux split-window -t $session -v
	tmux send-keys -t $session "$TAU -e 127.0.0.1:23342" Enter
fi
sleep 1
tmux new-window -t $session -n "node3"
tmux send-keys -t $session "$TAUD --config taud_full_node3.toml" Enter
if [ "$1" ]; then
	tmux split-window -t $session -v
	tmux send-keys -t $session "$TAU -e 127.0.0.1:23343" Enter
fi
sleep 1
tmux new-window -t $session -n "node4"
tmux send-keys -t $session "$TAUD --config taud_full_node4.toml" Enter
if [ "$1" ]; then
	tmux split-window -t $session -v
	tmux send-keys -t $session "$TAU -e 127.0.0.1:23344" Enter
fi
tmux attach -t $session
