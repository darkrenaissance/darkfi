#!/bin/sh
set -e
set -x

# Accept path to `drk` binary as arg or use default
DEFAULT_DRK="../../../drk -c drk0.toml"
DRK="${1:-$DEFAULT_DRK}"

# Path to contract to be deployed
WASM="../../../../smart-contract/membership.wasm"
CLIENT="../../../../smart-contract/membership"

# Script configuration
OUTPUT_FOLDER=/tmp/darkfi
mkdir -p $OUTPUT_FOLDER
SLEEP_TIME=5

# First run the darkfid nodes and the miners:
#
#   ./clean.sh
#   ./init-wallets.sh
#   ./tmux_sessions.sh
#
# Set the right path for WASM and CLIENT binaries for your membership contract.
# Now you can run this script

token_balance() {
    BALANCE="$($1 wallet balance 2>/dev/null)"

    # No tokens received at all yet
    if echo "$BALANCE" | grep -q "No unspent balances found"; then
        echo 0
        return
    fi

    BALANCE="$(echo "$BALANCE" | grep -q "$2")"
    # Not received yet so no entry
    if [ $? = 1 ]; then
        echo 0
        return
    fi

    # OK we have the token, show the actual balance
    echo "$BALANCE" | awk '{print $5}'
}

wait_token() {
    while [ "$(token_balance "$1" "$2")" = 0 ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh "$1" > /dev/null
    done
}

deployment_status() {
  STATUS="$($1 contract list $2)"

  # contract deployment hasn't been confirmed yet
  if echo "$STATUS" | grep -q "No history records found"; then
      echo 0
      return
  fi

  echo 1
}

wait_deployment() {
    while [ "$(deployment_status "$1" "$2")" = 0 ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh "$1" > /dev/null
    done
}

deploy_contract() {
  ADDRESS="$($1 contract generate-deploy | awk 'NR==3 {print $3}')"
  $1 contract deploy $ADDRESS $WASM | tee $OUTPUT_FOLDER/deploy-$ADDRESS.tx | $1 broadcast >/dev/null 2>&1
  echo "$ADDRESS"
}


tx_status() {
  STATUS="$($1 explorer txs-history | grep $2 | awk '{print $3}')"

  if echo "$STATUS" | grep -q "Broadcasted"; then
      echo 0
      return
  fi

  echo 1
}

wait_tx() {
    while [ "$(tx_status "$1" "$2")" = 0 ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh "$1" > /dev/null
    done
}

generate_key() {
  SECRET_KEY="$($CLIENT generate | awk 'NR==1 {print $3}')"
  echo "$SECRET_KEY"
}

register_call() {
  TX_ID="$($CLIENT register $2  $3 | tee $OUTPUT_FOLDER/register-$2-$3.call | $1 tx-from-calls | tee $OUTPUT_FOLDER/register-$2-$3.tx | $1 broadcast | awk 'NR==4 {print $3}')"
  echo "$TX_ID"
}

deregister_call() {
  TX_ID="$($CLIENT deregister $2  $3 | tee $OUTPUT_FOLDER/deregister-$2-$3.call | $1 tx-from-calls | tee $OUTPUT_FOLDER/deregister-$2-$3.tx | $1 broadcast | awk 'NR==4 {print $3}')"
  echo "$TX_ID"
}


wait_token "$DRK" DRK
CONTRACT_ADDRESS="$(deploy_contract "$DRK")"
wait_deployment "$DRK" "$CONTRACT_ADDRESS"

wait_token "$DRK" DRK
SECRET_KEY="$(generate_key "$DRK" "$CONTRACT_ADDRESS")"

TX_ID="$(register_call "$DRK" "$CONTRACT_ADDRESS" "$SECRET_KEY")"
wait_tx "$DRK" "$TX_ID"

TX_ID="$(deregister_call "$DRK" "$CONTRACT_ADDRESS" "$SECRET_KEY")"
wait_tx "$DRK" "$TX_ID"