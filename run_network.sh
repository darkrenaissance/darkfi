#!/bin/bash

# Run this script then
#   python scripts/monitor-p2p.py
# to view the network topology

declare -a arr=(
    # Seed node
    "cargo run --bin dfi -- -r 8999 --accept 127.0.0.1:9999 --log /tmp/darkfi/seed.log"
    # Server with no outgoing connections
    "cargo run --bin dfi -- -r 9000 --accept 127.0.0.1:10001 --seeds 127.0.0.1:9999 --log /tmp/darkfi/server.log"
    # Server/client with 2 outgoing connections
    "cargo run --bin dfi -- -r 9005 --accept 127.0.0.1:10002 --seeds 127.0.0.1:9999 --log /tmp/darkfi/server1.log --slots 3"
    # Server/client with 2 outgoing connections
    "cargo run --bin dfi -- -r 9006 --accept 127.0.0.1:10003 --seeds 127.0.0.1:9999 --log /tmp/darkfi/server2.log --slots 3"
    # Server/client with 2 outgoing connections
    "cargo run --bin dfi -- -r 9007 --accept 127.0.0.1:10004 --seeds 127.0.0.1:9999 --log /tmp/darkfi/server3.log --slots 3"
    # Server/client with 2 outgoing connections
    "cargo run --bin dfi -- -r 9008 --accept 127.0.0.1:10005 --seeds 127.0.0.1:9999 --log /tmp/darkfi/server4.log --slots 3"
    # Client with 1 outgoing connection
    "cargo run --bin dfi -- -r 9002 --seeds 127.0.0.1:9999 --slots 4 --log /tmp/darkfi/client.log"
    # Client with 1 outgoing connection
    "cargo run --bin dfi -- -r 9003 --seeds 127.0.0.1:9999 --slots 4 --log /tmp/darkfi/client1.log"
    # Client with 1 outgoing connection
    "cargo run --bin dfi -- -r 9004 --seeds 127.0.0.1:9999 --slots 4 --log /tmp/darkfi/client2.log"
)

mkdir -p /tmp/darkfi/

for cmd in "${arr[@]}"; do {
  echo "Process \"$cmd\" started";
  RUST_BACKTRACE=1 $cmd & pid=$!
  PID_LIST+=" $pid";
  sleep 2;
} done

trap "kill $PID_LIST" SIGINT

echo "Parallel processes have started";

wait $PID_LIST

echo
echo "All processes have completed";

