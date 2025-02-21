#!/bin/sh
set -e

# Start a tmux session with two damd nodes.
# Additionally, start the corresponding subscribers
# for each node and prepare a pane to start an attack.

# Path to used binaries
DAMD="../damd/damd"
DAMD_CLI="../dam-cli/dam-cli"
DAMD_CLI0="$DAMD_CLI -e tcp://127.0.0.1:44780"
DAMD_CLI1="$DAMD_CLI -e tcp://127.0.0.1:44880"

session=damd-localnet

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d -s $session -n "node0"
tmux send-keys -t $session "$DAMD $verbose -c damd0.toml" Enter
tmux new-window -t $session -n "node1"
tmux send-keys -t $session "$DAMD $verbose -c damd1.toml" Enter
sleep 1
tmux new-window -t $session -n "flood"
tmux send-keys -t $session "$DAMD_CLI0 subscribe protocols.subscribe_foo" Enter
tmux split-window -t $session -v -l 20%
tmux send-keys -t $session "$DAMD_CLI1 flood"
tmux select-pane -t 0
tmux split-window -t $session -h
tmux send-keys -t $session "$DAMD_CLI1 subscribe protocols.subscribe_attack_foo" Enter
tmux select-pane -t 0
tmux split-window -t $session -v
tmux send-keys -t $session "$DAMD_CLI0 subscribe protocols.subscribe_bar" Enter
tmux select-pane -t 2
tmux split-window -t $session -v
tmux send-keys -t $session "$DAMD_CLI1 subscribe protocols.subscribe_attack_bar" Enter
tmux select-pane -t 4
tmux attach -t $session
