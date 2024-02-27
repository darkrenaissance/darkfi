#!/bin/sh
set -e

# Start a tmux session with a minerd daemon and a darkfid node

session=darkfid-single-node

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d -s $session
tmux send-keys -t $session "../../../minerd ${verbose} -c minerd.toml" Enter
sleep 1
tmux split-window -t $session -v -l 90%
tmux send-keys -t $session "LOG_TARGETS='!sled,!net' ../../../darkfid ${verbose} -c darkfid.toml" Enter
tmux attach -t $session
