#!/bin/sh
set -e

# Start a tmux session with two mining and a non-mining darkfid nodes.
# Additionally, start two minerd daemons.

session=darkfid-small

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d -s $session
tmux send-keys -t $session "../../../minerd ${verbose} -c minerd0.toml" Enter
sleep 1
tmux split-window -t $session -v -l 90%
tmux send-keys -t $session "LOG_TARGETS='!sled,!runtime' ../../../darkfid ${verbose} -c darkfid0.toml" Enter
sleep 2
tmux new-window -t $session
tmux send-keys -t $session "../../../minerd ${verbose} -c minerd1.toml" Enter
sleep 1
tmux split-window -t $session -v -l 90%
tmux send-keys -t $session "LOG_TARGETS='!sled,!runtime' ../../../darkfid ${verbose} -c darkfid1.toml" Enter
sleep 2
tmux new-window -t $session
tmux send-keys -t $session "LOG_TARGETS='!sled,!runtime' ../../../darkfid ${verbose} -c darkfid2.toml" Enter
tmux attach -t $session
