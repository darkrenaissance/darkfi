#!/bin/sh
set -e

# Start a tmux session with five generic node daemons

session=five-generic-nodes

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d -s $session
tmux send-keys -t $session "cd .. && cargo +nightly run --release -- ${verbose} -c localnet/node0.toml --node-id 100" Enter
sleep 1
tmux new-window -t $session
tmux send-keys -t $session "cd .. && cargo +nightly run --release -- ${verbose} -c localnet/node1.toml --node-id 101" Enter
sleep 1
tmux new-window -t $session
tmux send-keys -t $session "cd .. && cargo +nightly run --release -- ${verbose} -c localnet/node2.toml --node-id 102" Enter
sleep 1
tmux new-window -t $session
tmux send-keys -t $session "cd .. && cargo +nightly run --release -- ${verbose} -c localnet/node3.toml --node-id 103" Enter
sleep 1
tmux new-window -t $session
tmux send-keys -t $session "cd .. && cargo +nightly run --release -- ${verbose} -c localnet/node4.toml --node-id 104" Enter
tmux attach -t $session
