#!/bin/sh
# Start a tmux session of 4 genev daemons, and 4 genev clis.
set -e

tmux new-session -s "genevd" -n "genevd" -d
tmux send-keys "../../../genevd --localnet --config genevd_seed.toml --skip-dag-sync" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../genevd --localnet --config genevd_a.toml --skip-dag-sync" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../genevd --localnet --config genevd_b.toml --skip-dag-sync" Enter
tmux select-pane -t 0
tmux split-window -v
tmux send-keys "../../../genevd --localnet --config genevd_c.toml" Enter
tmux select-pane -t 2
tmux split-window -v
tmux send-keys "../../../genevd --localnet --config genevd_d.toml" Enter


tmux new-window -t "genevd:1" -n "genev"
sleep 5
tmux send-keys "../../../genev -e tcp://127.0.0.1:28870 add alolymous \"pay bills\" \"gonna pay some bills in the morning\" " Enter
tmux split-window -v
sleep 1
tmux send-keys "../../../genev -e tcp://127.0.0.1:28871 list" Enter
tmux split-window -h
tmux send-keys "../../../genev -e tcp://127.0.0.1:28872 list" Enter
tmux select-pane -t 0
tmux split-window -h
tmux send-keys "../../../genev -e tcp://127.0.0.1:28873 list" Enter


tmux attach
