#!/bin/sh

# Path to `drk` binary
DRK="../../../drk -c drk.toml"

$DRK wallet initialize
$DRK wallet keygen
$DRK wallet default-address 1
wallet=$($DRK wallet address)
sed -i -e "s|9vw6WznKk7xEFQwwXhJWMMdjUPi3cXL8NrFKQpKifG1U|$wallet|g" minerd.toml
