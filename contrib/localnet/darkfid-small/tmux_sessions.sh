#!/bin/sh
set -e

# Start a tmux session with two mining and a non-mining darkfid nodes.
# Additionally, start two xmrig daemons.

# Path to used binaries
XMRIG="xmrig"
DARKFID="LOG_TARGETS='!runtime,!sled' ../../../darkfid"

# Dummy mining config wallet so mining daemons can start.
XMRIG_USER="OERjbThtVW1VMkZIYmI2RlhucUx0OXByaFRSWmVWcE5hdTROWXQ3Szg1ZDVVWnA0RGpabmFKZVZEAAA"

session=darkfid-small

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d -s $session -n "node0"
tmux send-keys -t $session "$XMRIG -u x+1 -r 1000 -R 20 -o 127.0.0.1:48241 -t 2 -u $XMRIG_USER" Enter
tmux split-window -t $session -v -l 80%
tmux send-keys -t $session "$DARKFID -c darkfid0.toml $verbose" Enter
tmux new-window -t $session -n "node1"
tmux send-keys -t $session "$XMRIG -u x+1 -r 1000 -R 20 -o 127.0.0.1:48341 -t 2 -u $XMRIG_USER" Enter
tmux split-window -t $session -v -l 80%
tmux send-keys -t $session "$DARKFID -c darkfid1.toml $verbose" Enter
tmux new-window -t $session -n "node2"
tmux send-keys -t $session "$DARKFID -c darkfid2.toml $verbose" Enter
tmux attach -t $session
