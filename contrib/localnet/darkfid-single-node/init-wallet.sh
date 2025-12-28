#!/bin/sh

# Path to `drk` binary
DRK="../../../drk -c drk.toml"

$DRK wallet initialize
$DRK wallet keygen
$DRK wallet default-address 1
wallet=$($DRK wallet mining-config 1 | tail -n 1)
sed -i -e "s|OERjbThtVW1VMkZIYmI2RlhucUx0OXByaFRSWmVWcE5hdTROWXQ3Szg1ZDVVWnA0RGpabmFKZVZEAAA|$wallet|g" tmux_sessions.sh
