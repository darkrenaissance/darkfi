#!/bin/sh
set -e

# Start a tmux session with two mining and a non-mining darkfid nodes.
# Additionally, start two minerd daemons.

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d
tmux send-keys "../../../minerd ${verbose} -c minerd0.toml" Enter
sleep 1
tmux split-window -v
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid0.toml" Enter
sleep 2
tmux new-window
tmux send-keys "../../../minerd ${verbose} -c minerd1.toml" Enter
sleep 1
tmux split-window -v
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid1.toml" Enter
sleep 2
tmux new-window
tmux send-keys "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid2.toml" Enter
tmux attach
