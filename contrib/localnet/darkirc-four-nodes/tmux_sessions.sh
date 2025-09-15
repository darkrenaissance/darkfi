#!/bin/sh
set -e

# Start a tmux session of four darkirc nodes, and optionally four weechat clients.

# Path to used binaries
DARKIRC="../../../darkirc"
WEECHAT="weechat -t -r"

session=darkirc-local

tmux new-session -d -s $session -n "seed"
tmux send-keys -t $session "$DARKIRC -c seed.toml --skip-dag-sync" Enter
sleep 1
tmux new-window -t $session -n "node1"
tmux send-keys -t $session "$DARKIRC -c darkirc_full_node1.toml --skip-dag-sync" Enter
if [ "$1" ]; then
	tmux split-window -t $session -v
	tmux send-keys -t $session "$WEECHAT '/server add darkirc_a 127.0.0.1/22022 -notls;/connect darkirc_a;/set irc.server_default.nicks Alice'" Enter
fi
sleep 1
tmux new-window -t $session -n "node2"
tmux send-keys -t $session "$DARKIRC -c darkirc_full_node2.toml" Enter
if [ "$1" ]; then
	tmux split-window -t $session -v
	tmux send-keys -t $session "$WEECHAT '/server add darkirc_b 127.0.0.1/22023 -notls;/connect darkirc_b;/set irc.server_default.nicks Bob'" Enter
fi
sleep 1
tmux new-window -t $session -n "node3"
tmux send-keys -t $session "$DARKIRC -c darkirc_full_node3.toml" Enter
if [ "$1" ]; then
	tmux split-window -t $session -v
	tmux send-keys -t $session "$WEECHAT '/server add darkirc_c 127.0.0.1/22024 -notls;/connect darkirc_c;/set irc.server_default.nicks Charlie'" Enter
fi
sleep 1
tmux new-window -t $session -n "node4"
tmux send-keys -t $session "$DARKIRC -c darkirc_full_node4.toml --fast-mode" Enter
if [ "$1" ]; then
	tmux split-window -t $session -v
	tmux send-keys -t $session "$WEECHAT '/server add darkirc_d 127.0.0.1/22025 -notls;/connect darkirc_d;/set irc.server_default.nicks Dave'" Enter
fi
tmux attach -t $session
