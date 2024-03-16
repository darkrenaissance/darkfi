#!/bin/sh
# Start a tmux session of four dchatd nodes.
set -e

tmux new-session -s "dchatd" -n "dchatd" -d
tmux send-keys "../../../target/release/dchatd --config seed.toml -vv 2>&1 | tee seed.log" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../target/release/dchatd --config dchat1.toml -vv 2>&1 | tee dchat1.log" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../target/release/dchatd --config dchat2.toml -vv 2>&1 | tee dchat2.log" Enter
tmux select-pane -t 0
tmux split-window -v
tmux send-keys "../../../target/release/dchatd --config dchat3.toml -vv 2>&1 | tee dchat3.log" Enter
tmux select-pane -t 2
tmux split-window -v
tmux send-keys "../../../target/release/dchatd --config dchat4.toml -vv 2>&1 | tee dchat4.log" Enter

tmux attach
