#!/bin/sh
set -e

# Start a tmux session with an xmrig daemon and a darkfid node

# xmrig configuration
XMRIG_BINARY_PATH="xmrig"
XMRIG_STRATUM_ENDPOINT="127.0.0.1:48241"
XMRIG_THREADS="4"
XMRIG_USER="OERjbThtVW1VMkZIYmI2RlhucUx0OXByaFRSWmVWcE5hdTROWXQ3Szg1ZDVVWnA0RGpabmFKZVZEAAA"
XMRIG_PARAMS="-u x+1 -o $XMRIG_STRATUM_ENDPOINT -t $XMRIG_THREADS -u $XMRIG_USER"
XMRIG="$XMRIG_BINARY_PATH $XMRIG_PARAMS"

# Path to darkfid binary
DARKFID="LOG_TARGETS='!net,!runtime,!sled' ../../../darkfid -c darkfid.toml"

session=darkfid-single-node

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d -s $session -n $session
tmux send-keys -t $session "$XMRIG" Enter
tmux split-window -t $session -v -l 80%
tmux send-keys -t $session "$DARKFID $verbose" Enter
tmux attach -t $session
