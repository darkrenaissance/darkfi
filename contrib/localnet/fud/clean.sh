#!/bin/sh
rm -rf fud0 fud1 lilith_hosts.tsv
mkdir fud0
mkdir fud1
echo "Hello from node 0." > "fud0/node0"
echo "Hello from node 1." > "fud1/node1"
