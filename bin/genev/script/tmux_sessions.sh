#!/bin/sh
# Start a tmux session of 4 genev daemons, and 4 genev clis.
set -e

tmux new-session -s "genevd" -n "genevd" -d
tmux send-keys "../../../genevd --localnet --config genevd_seed.toml" Enter && sleep 1
tmux split-window -h
tmux send-keys "../../../genevd --localnet --config genevd_a.toml" Enter
tmux split-window -h
tmux send-keys "../../../genevd --localnet --config genevd_b.toml" Enter
tmux select-pane -t 0
tmux split-window -v
tmux send-keys "../../../genevd --localnet --config genevd_c.toml" Enter
tmux select-pane -t 2
tmux split-window -v
tmux send-keys "../../../genevd --localnet --config genevd_d.toml" Enter


tmux new-window -t "genevd:1" -n "genev"
tmux send-keys "../../../genev -e tcp://127.0.0.1:28870" Enter
tmux split-window -v
tmux send-keys "../../../genev -e tcp://127.0.0.1:28871" Enter
tmux split-window -h
tmux send-keys "../../../genev -e tcp://127.0.0.1:28872" Enter
tmux select-pane -t 0
tmux split-window -h
tmux send-keys "../../../genev -e tcp://127.0.0.1:28873" Enter


tmux attach
