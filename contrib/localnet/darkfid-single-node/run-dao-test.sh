#!/bin/sh
set -e
set -x

# Path to `drk` binary
DRK="../../../drk -c drk.toml"

# Script configuration
OUTPUT_FOLDER=/tmp/darkfi
mkdir -p $OUTPUT_FOLDER
SLEEP_TIME=5

# First run the darkfid node and the miner:
#
#   ./clean.sh
#   ./init-wallet.sh
#   ./tmux_sessions.sh
#
# Now you can run this script

mint_token() {
    $DRK alias add $1 "$($DRK token generate-mint | awk '{print $8}')"
    $DRK token mint $1 $2 "$($DRK wallet address)" | tee $OUTPUT_FOLDER/mint-$1.tx | $DRK broadcast
    $DRK token list
}

token_balance() {
    BALANCE="$($DRK wallet balance 2>/dev/null)"

    # No tokens received at all yet
    if echo "$BALANCE" | grep -q "No unspent balances found"; then
        echo 0
        return
    fi

    BALANCE="$(echo "$BALANCE" | grep -q "$1")"
    # Not received yet so no entry
    if [ $? = 1 ]; then
        echo 0
        return
    fi

    # OK we have the token, show the actual balance
    echo "$BALANCE" | awk '{print $5}'
}

wait_token() {
    while [ "$(token_balance $1)" = 0 ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh > /dev/null
    done
}

mint_dao() {
    $DRK dao create 20 10 10 0.67 MLDY > $OUTPUT_FOLDER/dao.toml
    $DRK dao import MiladyMakerDAO < $OUTPUT_FOLDER/dao.toml
    $DRK dao list
    $DRK dao list MiladyMakerDAO

    $DRK dao mint MiladyMakerDAO | tee $OUTPUT_FOLDER/dao-mint.tx | $DRK broadcast
}

wait_dao_mint() {
    while [ "$($DRK dao list MiladyMakerDAO | grep '^Transaction hash: ' | awk '{print $3}')" = None ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh > /dev/null
    done
}

fill_treasury() {
    PUBKEY="$($DRK dao list MiladyMakerDAO | grep '^Notes Public key: ' | cut -d ' ' -f4)"
    SPEND_HOOK="$($DRK dao spend-hook)"
    BULLA="$($DRK dao list MiladyMakerDAO | grep '^Bulla: ' | cut -d' ' -f2)"
    $DRK transfer 20 WCKD "$PUBKEY" "$SPEND_HOOK" "$BULLA" | tee $OUTPUT_FOLDER/xfer.tx | $DRK broadcast
}

dao_balance() {
    BALANCE=$($DRK dao balance MiladyMakerDAO 2>/dev/null)
    # No tokens received at all yet
    if echo "$BALANCE" | grep -q "No unspent balances found"; then
        echo 0
        return
    fi

    BALANCE=$(echo "$BALANCE" | grep "$1")
    # Not received yet so no entry
    if [ $? = 1 ]; then
        echo 0
        return
    fi

    # OK we have the token, show the actual balance
    echo "$BALANCE" | awk '{print $5}'
}

wait_dao_treasury() {
    while [ "$(dao_balance WCKD)" = 0 ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh > /dev/null
    done
}

propose() {
    MY_ADDR=$($DRK wallet address)
    PROPOSAL="$($DRK dao propose-transfer MiladyMakerDAO 1 5 WCKD "$MY_ADDR" | cut -d' ' -f3)"
    $DRK dao proposal "$PROPOSAL" --mint-proposal | tee $OUTPUT_FOLDER/propose.tx | $DRK broadcast
}

wait_proposal() {
    PROPOSAL="$($DRK dao proposals MiladyMakerDAO | cut -d' ' -f2)"
    while [ "$($DRK dao proposal $PROPOSAL | grep '^Proposal transaction hash: ' | awk '{print $4}')" = None ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh > /dev/null
    done
}

vote() {
    PROPOSAL="$($DRK dao proposals MiladyMakerDAO | cut -d' ' -f2)"
    $DRK dao vote "$PROPOSAL" 1 | tee $OUTPUT_FOLDER/dao-vote.tx | $DRK broadcast
}

wait_vote() {
    PROPOSAL="$($DRK dao proposals MiladyMakerDAO | cut -d' ' -f2)"
    while [ "$($DRK dao proposal $PROPOSAL | grep '^Current proposal outcome: ' | awk '{print $4}')" != "Approved" ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh > /dev/null
    done
}

do_exec() {
    PROPOSAL="$($DRK dao proposals MiladyMakerDAO | cut -d' ' -f2)"
    $DRK dao exec --early $PROPOSAL | tee $OUTPUT_FOLDER/dao-exec.tx | $DRK broadcast
}

wait_exec() {
    PROPOSAL="$($DRK dao proposals MiladyMakerDAO | cut -d' ' -f2)"
    while [ -z "$($DRK dao proposal $PROPOSAL | grep '^Proposal was executed on transaction: ')" ]; do
        sleep $SLEEP_TIME
        sh ./sync-wallet.sh > /dev/null
    done
}

wait_token DRK
mint_token WCKD 42
wait_token WCKD
mint_token MLDY 20
wait_token MLDY
mint_dao
wait_dao_mint
fill_treasury
wait_dao_treasury
propose
wait_proposal
vote
wait_vote
do_exec
wait_exec
