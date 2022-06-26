#!/bin/bash
LOCAL_IP=$(ip route get 8.8.8.8 | head -1 | awk '{print $7}')
cargo run -- --accept 0.0.0.0:9999 --irc 127.0.0.1:6688

