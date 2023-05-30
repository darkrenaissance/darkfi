#!/bin/sh
set -e
set -x

# Path to `drk` binary
DRK="../../../drk"

while true; do
    if $DRK ping 2> /dev/null; then
        break
    fi
    sleep 1
done

$DRK wallet --initialize
$DRK scan
$DRK subscribe blocks

