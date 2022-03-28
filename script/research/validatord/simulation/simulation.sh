#!/bin/bash

# Simulation of the consensus network for 4(hardcoded) validator nodes.

# Copying the node state files with a blockchain containing only the genesis block.
for i in {0..3}
do
  cp validatord_state_$i ~/.config/darkfi/validatord_state_$i
done

# Starting node 0 (seed) in background
cargo +nightly run -- &
NODE0=$!

# Waiting for seed to setup
sleep 10

# Starting node 1 in background
cargo +nightly run -- --accept 0.0.0.0:11001 --seeds 127.0.0.1:11000 --rpc 127.0.0.1:6661 --external 127.0.0.1:11001 --id 1 --state ~/.config/darkfi/validatord_state_1 &
NODE1=$!

# Waiting for node 1 to setup
sleep 5

# Starting node 2 in background
cargo +nightly run -- --accept 0.0.0.0:11002 --seeds 127.0.0.1:11000 --rpc 127.0.0.1:6662 --external 127.0.0.1:11002 --id 2 --state ~/.config/darkfi/validatord_state_2 &
NODE2=$!

# Waiting for node 2 to setup
sleep 5

# Trap kill signal
trap ctrl_c INT

# On kill signal, terminate background node processes
function ctrl_c() {
    kill $NODE0
    kill $NODE1
    kill $NODE2
}

# Starting node 3
cargo +nightly run -- --accept 0.0.0.0:11003 --seeds 127.0.0.1:11000 --rpc 127.0.0.1:6663 --external 127.0.0.1:11003 --id 3 --state ~/.config/darkfi/validatord_state_3

# Node states are flushed on each node state file at epoch end (every 2 minutes).
# To sugmit a TX, telnet to a node and push the json as per following example:
# telnet 127.0.0.1 6661
# {"jsonrpc": "2.0", "method": "receive_tx", "params": ["tx"], "id": 42}
