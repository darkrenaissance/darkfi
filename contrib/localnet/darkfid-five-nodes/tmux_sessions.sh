#!/bin/sh
set -e

# Start a tmux session with five xmrig daemons and five darkfid nodes

# Path to used binaries
XMRIG="xmrig"
DARKFID="LOG_TARGETS='!runtime,!sled' ../../../darkfid"

# Dummy mining wallet address so mining daemons can start
XMRIG_USER="DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"

session=darkfid-five-nodes

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d -s $session -n "node0"
tmux send-keys -t $session "$XMRIG -u x+1 -r 1000 -R 20 -o 127.0.0.1:48347 -t 2 -u $XMRIG_USER" Enter
tmux split-window -t $session -v -l 80%
tmux send-keys -t $session "$DARKFID -c darkfid0.toml $verbose" Enter
tmux new-window -t $session -n "node1"
tmux send-keys -t $session "$XMRIG -u x+1 -r 1000 -R 20 -o 127.0.0.1:48447 -t 2 -u $XMRIG_USER" Enter
tmux split-window -t $session -v -l 80%
tmux send-keys -t $session "$DARKFID -c darkfid1.toml $verbose" Enter
tmux new-window -t $session -n "node2"
tmux send-keys -t $session "$XMRIG -u x+1 -r 1000 -R 20 -o 127.0.0.1:48547 -t 2 -u $XMRIG_USER" Enter
tmux split-window -t $session -v -l 80%
tmux send-keys -t $session "$DARKFID -c darkfid2.toml $verbose" Enter
tmux new-window -t $session -n "node3"
tmux send-keys -t $session "$XMRIG -u x+1 -r 1000 -R 20 -o 127.0.0.1:48647 -t 2 -u $XMRIG_USER" Enter
tmux split-window -t $session -v -l 80%
tmux send-keys -t $session "$DARKFID -c darkfid3.toml $verbose" Enter
tmux new-window -t $session -n "node4"
tmux send-keys -t $session "$XMRIG -u x+1 -r 1000 -R 20 -o 127.0.0.1:48747 -t 2 -u $XMRIG_USER" Enter
tmux split-window -t $session -v -l 80%
tmux send-keys -t $session "$DARKFID -c darkfid4.toml $verbose" Enter
tmux attach -t $session
