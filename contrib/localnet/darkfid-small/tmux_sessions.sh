#!/bin/sh
set -e

# Start a tmux session with two mining and a non-mining darkfid nodes.
# Additionally, start two xmrig daemons.

# Generate each node folder
mkdir -p darkfid0 darkfid1 darkfid2

# Path to used binaries
XMRIG="xmrig"
DARKFID="LOG_TARGETS='!sled' ../../../darkfid"

# Dummy mining wallet addresses so mining daemons can start
XMRIG_USER0="DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"
XMRIG_USER1="Dae4FtyzrnQ8JNuui5ibZL4jXUR786PbyjwBsq4aj6E1RPPYjtXLfnAf"

session=darkfid-small

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi

tmux new-session -d -s $session -n "node0"
tmux send-keys -t $session "$XMRIG -u x+1 -r 1000 -R 20 -o 127.0.0.1:48347 -t 2 -u $XMRIG_USER0" Enter
tmux split-window -t $session -v -l 80%
tmux send-keys -t $session "$DARKFID -c darkfid0.toml $verbose -l darkfid0/darkfid.log" Enter
tmux new-window -t $session -n "node1"
tmux send-keys -t $session "$XMRIG -u x+1 -r 1000 -R 20 -o 127.0.0.1:48447 -t 2 -u $XMRIG_USER1" Enter
tmux split-window -t $session -v -l 80%
tmux send-keys -t $session "$DARKFID -c darkfid1.toml $verbose -l darkfid1/darkfid.log" Enter
tmux new-window -t $session -n "node2"
tmux send-keys -t $session "$DARKFID -c darkfid2.toml $verbose -l darkfid2/darkfid.log" Enter
tmux attach -t $session
