#!/bin/sh
set -e

# Start a tmux session with two mining and a non-mining darkfid nodes.
# Additionally, start two xmrig daemons.
#

# Path to used binaries
XMRIG="xmrig"
DARKFID="LOG_TARGETS='!sled' ../../../darkfid"

# Dummy mining wallet addresses so mining daemons can start
XMRIG_USER0="DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"
XMRIG_USER1="Dae4FtyzrnQ8JNuui5ibZL4jXUR786PbyjwBsq4aj6E1RPPYjtXLfnAf"

# Path to `drk` binary
DRK="../../../drk"
DRK0="$DRK -c drk0.toml"
DRK1="$DRK -c drk1.toml"

# First run the darkfid nodes and the miners:
#
#   ./clean.sh
#   ./init-wallets.sh
#   ./tmux_sessions.sh
#
# Wait for at least 20 blocks to be created.
# Now you can run this script

session=darkfid-small

if [ "$1" = "-vv" ]; then
	verbose="-vv"
	shift
else
	verbose=""
fi


echo "=========[Stopping Node1 and Node2]=========="
# Stop node1 and mine only with node0
tmux send-keys -t "$session:node1.0" C-c
tmux send-keys -t "$session:node1.1" C-c
tmux send-keys -t "$session:node2.0" C-c
sleep 5

# Create contract txs on node0
sh ./run-contract-test.sh "$DRK0"

# Stop node0 and mine only with node1
echo "===========[Stopping Node0]=================="
tmux send-keys -t "$session:node0.0" C-c
tmux send-keys -t "$session:node0.1" C-c
sleep 5
echo "===========[Restarting Node1]================"
sed -i -e "s|skip_sync =.*|skip_sync = true|g" darkfid1.toml
tmux send-keys -t "$session:node1.1" "$DARKFID -c darkfid1.toml $verbose -l darkfid1.log" Enter
tmux send-keys -t "$session:node1.0" "$XMRIG -u x+1 -r 1000 -R 20 -o 127.0.0.1:48447 -t 2 -u $XMRIG_USER1" Enter
sleep 2

# Create contract txs on node1
sh ./run-contract-test.sh "$DRK1"

# Restart node0 and see reorg happening
echo "=========[Restarting Node0 and Node2]========"
tmux send-keys -t "$session:node0.1" "$DARKFID -c darkfid0.toml $verbose -l darkfid0.log" Enter
tmux send-keys -t "$session:node0.0" "$XMRIG -u x+1 -r 1000 -R 20 -o 127.0.0.1:48347 -t 2 -u $XMRIG_USER0" Enter
tmux send-keys -t "$session:node2.0" "$DARKFID -c darkfid2.toml $verbose -l darkfid2.log" Enter
sleep 2

# Test to check everything is working fine
sh ./run-contract-test.sh "$DRK0"
sh ./run-contract-test.sh "$DRK1"