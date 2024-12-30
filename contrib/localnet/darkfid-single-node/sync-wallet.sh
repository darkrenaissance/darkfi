#!/bin/sh
set -e
set -x

# Path to `drk` binary
DRK="../../../drk -c drk.toml"

while true; do
    if $DRK ping 2> /dev/null; then
        break
    fi
    sleep 1
done

$DRK scan
$DRK subscribe
