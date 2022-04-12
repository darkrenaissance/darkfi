#!/bin/sh

tmux new-session -d './target/release/p2pdebug'
tmux split-window -v './target/release/p2pdebug -n 3 '
tmux split-window -h './target/release/p2pdebug -n 21 -b'
tmux attach 
