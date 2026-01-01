#!/bin/sh

# Path to `drk` binary
DRK="../../../drk -c drk.toml"

$DRK wallet initialize
$DRK wallet keygen
$DRK wallet default-address 1
wallet=$($DRK wallet address)
sed -i -e "s|DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf|$wallet|g" tmux_sessions.sh
