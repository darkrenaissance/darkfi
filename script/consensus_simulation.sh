#!/bin/bash

# Simulation of the consensus network for n(>2) nodes.

nodes=4

# moving one folder up
cd ..

# compiling bin
make BINS=darkfid2

# PIDs array
pids=()

# Starting node 0 (seed) in background
LOG_TARGETS="!sled,!net" ./darkfid2 \
    -v \
    --consensus \
    --consensus-p2p-accept 127.0.0.1:6000 \
    --consensus-p2p-external 127.0.0.1:6000 \
    --database ./tmp/node0/blockchain \
    --rpc-listen tcp://127.0.0.1:6010 \
    --sync-p2p-accept 127.0.0.1:6020 \
    --sync-p2p-external 127.0.0.1:6020 \
    --wallet-path ./tmp/node0/wallet.db &
    
pids[${#pids[@]}]=$!

# Waiting for seed to setup
sleep 20

# Starting nodes 1 till second to last node in background
bound=$(($nodes-2))
for i in $(eval echo "{1..$bound}")
do
  LOG_TARGETS="!sled,!net" ./darkfid2 \
    -v \
    --consensus \
    --consensus-p2p-seed 127.0.0.1:6000 \
    --sync-p2p-seed 127.0.0.1:6020 \
    --consensus-p2p-accept 127.0.0.1:600$i \
    --consensus-p2p-external 127.0.0.1:600$i \
    --database ./tmp/node$i/blockchain \
    --rpc-listen tcp://127.0.0.1:601$i \
    --sync-p2p-accept 127.0.0.1:602$i \
    --sync-p2p-external 127.0.0.1:602$i \
    --wallet-path ./tmp/node$i/wallet.db &
  pids[${#pids[@]}]=$!
  # waiting for node to setup
  sleep 20
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
LOG_TARGETS="!sled,!net" ./darkfid2 \
    -v \
    --consensus \
    --consensus-p2p-seed 127.0.0.1:6000 \
    --sync-p2p-seed 127.0.0.1:6020 \
    --consensus-p2p-accept 127.0.0.1:600$bound \
    --consensus-p2p-external 127.0.0.1:600$bound \
    --database ./tmp/node$bound/blockchain \
    --rpc-listen tcp://127.0.0.1:601$bound \
    --sync-p2p-accept 127.0.0.1:602$bound \
    --sync-p2p-external 127.0.0.1:602$bound \
    --wallet-path ./tmp/node$bound/wallet.db
