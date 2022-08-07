#!/bin/sh
export LOG_TARGETS='raft' 

tmux new-session -d "./target/release/raft-diag --inbound tcp://127.0.0.1:12001 --path test1.db -v"
sleep 3
tmux split-window -v "./target/release/raft-diag --inbound tcp://127.0.0.1:12002 --seeds tcp://127.0.0.1:12001 --outbound 3 --path test2.db -v "
sleep 2
tmux split-window -h "./target/release/raft-diag  --seeds tcp://127.0.0.1:12001 --outbound 3 --path test3.db -v"
sleep 1
tmux select-pane -t 0
tmux split-window -h "./target/release/raft-diag  --seeds tcp://127.0.0.1:12001 --outbound 3 --path test4.db -b 3 -v"
tmux attach
