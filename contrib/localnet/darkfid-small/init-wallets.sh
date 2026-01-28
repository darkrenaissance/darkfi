#!/bin/sh

# Path to `drk` binary
DRK="../../../drk"
DRK0="$DRK -c drk0.toml"
DRK1="$DRK -c drk1.toml"

initialize() {
  $1 wallet initialize
  $1 wallet keygen
  $1 wallet default-address 1
  wallet=$($1 wallet address)
  sed -i -e "s|$2|$wallet|g" tmux_sessions.sh
  sed -i -e "s|$2|$wallet|g" reorg-test.sh
}

initialize "$DRK0" "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"
initialize "$DRK1" "Dae4FtyzrnQ8JNuui5ibZL4jXUR786PbyjwBsq4aj6E1RPPYjtXLfnAf"
