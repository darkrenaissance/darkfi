#!/bin/sh

tmux new-session -d 'LOG_TARGETS=net ../../../target/release/ircd -vv --accept 127.0.0.1:9999 --irc 127.0.0.1:6688'
tmux split-window -v 'LOG_TARGETS=net ../../../target/release/ircd -vv --accept 127.0.0.1:11004 --external 127.0.0.1:11004 --seeds 127.0.0.1:9999 --irc 127.0.0.1:6667'
tmux split-window -h 'LOG_TARGETS=net ../../../target/release/ircd -vv --slots 5 --seeds 127.0.0.1:9999 --irc 127.0.0.1:6668'
tmux attach 
