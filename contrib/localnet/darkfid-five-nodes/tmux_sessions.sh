#!/bin/sh
set -e

# Start a tmux session with five minerd daemons and five darkfid nodes

session=darkfid-five-nodes

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d -s $session
tmux send-keys -t $session "../../../minerd ${verbose} -c minerd0.toml" Enter
sleep 1
tmux split-window -t $session -v
tmux send-keys -t $session "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid0.toml" Enter
sleep 2
tmux new-window -t $session
tmux send-keys -t $session "../../../minerd ${verbose} -c minerd1.toml" Enter
sleep 1
tmux split-window -t $session -v
tmux send-keys -t $session "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid1.toml" Enter
sleep 2
tmux new-window -t $session
tmux send-keys -t $session "../../../minerd ${verbose} -c minerd2.toml" Enter
sleep 1
tmux split-window -t $session -v
tmux send-keys -t $session "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid2.toml" Enter
sleep 2
tmux new-window -t $session
tmux send-keys -t $session "../../../minerd ${verbose} -c minerd3.toml" Enter
sleep 1
tmux split-window -t $session -v
tmux send-keys -t $session "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid3.toml" Enter
sleep 2
tmux new-window -t $session
tmux send-keys -t $session "../../../minerd ${verbose} -c minerd4.toml" Enter
sleep 1
tmux split-window -t $session -v
tmux send-keys -t $session "LOG_TARGETS='!sled' ../../../darkfid ${verbose} -c darkfid4.toml" Enter
tmux attach -t $session
