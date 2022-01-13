#!/bin/bash
LOCAL_IP=$(ip route get 8.8.8.8 | head -1 | awk '{print $7}')
SEED_IP=$(getent hosts XXX.local | awk '{print $1}' | head -n 1)
cargo run -- --accept 0.0.0.0:11004 --slots 5 --external $LOCAL_IP:11004 --seeds $SEED_IP:9999 --irc 127.0.0.1:6667

