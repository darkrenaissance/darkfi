#!/bin/bash

# Simulation of the consensus network for n validator nodes.
# Note: state files should be generated before executing.

nodes=10

# Copying the node state files with a blockchain containing only the genesis block.
bound=$(($nodes - 1))
for i in $(eval echo "{0..$bound}")
do
  cp validatord_state_$i ~/.config/darkfi/validatord_state_$i
done

# PIDs array
pids=()

# Starting node 0 (seed) in background
cargo run -- &
pids[${#pids[@]}]=$!

# Waiting for seed to setup
sleep 2

# Starting nodes 1 till second to last node in background
bound=$(($nodes-2))
for i in $(eval echo "{1..$bound}")
do
  cargo run -- --accept 0.0.0.0:1100$i --seeds 127.0.0.1:11000 --rpc 127.0.0.1:666$i --external 127.0.0.1:1100$i --id $i --state ~/.config/darkfi/validatord_state_$i &
  pids[${#pids[@]}]=$!
  # waiting for node to setup
  sleep 2
done

# Trap kill signal
trap ctrl_c INT

# On kill signal, terminate background node processes
function ctrl_c() {
    for pid in ${pids[@]}
    do
      kill $pid
    done
}

bound=$(($nodes-1))
# Starting last node
cargo run -- --accept 0.0.0.0:1100$bound --seeds 127.0.0.1:11000 --rpc 127.0.0.1:666$bound --external 127.0.0.1:1100$bound --id $bound --state ~/.config/darkfi/validatord_state_$bound

# Node states are flushed on each node state file at epoch end (every 2 minutes).
# To sugmit a TX, telnet to a node and push the json as per following example:
# telnet 127.0.0.1 6661
# {"jsonrpc": "2.0", "method": "receive_tx", "params": ["tx"], "id": 42}
