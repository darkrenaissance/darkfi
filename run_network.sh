#!/bin/bash

# Run this script then
#   python scripts/monitor-p2p.py
# to view the network topology

declare -a arr=(
    # Seed node
    "cargo run --bin dfi -- -r 8999 --accept 127.0.0.1:9999 -D --log /tmp/darkfi/seed.log"
    # Server with no outgoing connections
    "cargo run --bin dfi -- -r 9000 --accept 127.0.0.1:10001 --seeds 127.0.0.1:9999 --log /tmp/darkfi/server.log"
    # Client with 1 outgoing connection
    "cargo run --bin dfi -- -r 9002 --seeds 127.0.0.1:9999 --slots 1 --log /tmp/darkfi/client.log"
    # Client with 1 outgoing connection
    "cargo run --bin dfi -- -r 9003 --seeds 127.0.0.1:9999 --slots 1 --log /tmp/darkfi/client1.log"
    # Client with 1 outgoing connection
    "cargo run --bin dfi -- -r 9004 --seeds 127.0.0.1:9999 --slots 1 --log /tmp/darkfi/client2.log"
    # Server/client with 2 outgoing connections
    "cargo run --bin dfi -- -r 9005 --accept 127.0.0.1:10002 --seeds 127.0.0.1:9999 --log /tmp/darkfi/server1.log" --slots 2
)

for cmd in "${arr[@]}"; do {
  echo "Process \"$cmd\" started";
  RUST_BACKTRACE=1 $cmd & pid=$!
  PID_LIST+=" $pid";
} done

trap "kill $PID_LIST" SIGINT

echo "Parallel processes have started";

wait $PID_LIST

echo
echo "All processes have completed";

