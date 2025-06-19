#!/bin/sh
set -e
set -x

# Path to `drk` binary
DRK="../../../drk -c drk.toml"

# First run the darkfid node and the miner:
#
#   ./clean.sh
#   ./init-wallet.sh
#   ./tmux_sessions.sh -vv
#
# In another term run the wallet syncing,
# and wait until some blocks are mined:
#
#   ./sync-wallet.sh
#
# Finally you can run this script

mint_tokens() {
    $DRK token generate-mint
    $DRK token generate-mint

    TOKEN_ID1="$($DRK token list 2>/dev/null | awk 'NR == 3 {print $1}')"
    TOKEN_ID2="$($DRK token list 2>/dev/null | awk 'NR == 4 {print $1}')"

    $DRK alias add WCKD "$TOKEN_ID1"
    $DRK alias add MLDY "$TOKEN_ID2"

    ADDR="$($DRK wallet address)"

    $DRK token mint WCKD 42 "$ADDR" | tee /tmp/mint-wkcd.tx | $DRK broadcast
    $DRK token mint MLDY 20 "$ADDR" | tee /tmp/mint-mldy.tx | $DRK broadcast

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

wait_tokens() {
    while [ "$(token_balance WCKD)" = 0 ] || [ "$(token_balance MLDY)" = 0 ]; do
        sleep 1
    done
}

mint_dao() {
    $DRK dao create 20 10 10 0.67 MLDY > /tmp/dao.toml
    $DRK dao import MiladyMakerDAO < /tmp/dao.toml
    $DRK dao list
    $DRK dao list MiladyMakerDAO

    $DRK dao mint MiladyMakerDAO | tee /tmp/dao-mint.tx | $DRK broadcast
}

wait_dao_mint() {
    while [ "$($DRK dao list MiladyMakerDAO | grep '^Transaction hash: ' | awk '{print $3}')" = None ]; do
        sleep 1
    done
}

fill_treasury() {
    PUBKEY="$($DRK dao list MiladyMakerDAO | grep '^Notes Public key: ' | cut -d ' ' -f4)"
    SPEND_HOOK="$($DRK dao spend-hook)"
    BULLA="$($DRK dao list MiladyMakerDAO | grep '^Bulla: ' | cut -d' ' -f2)"
    $DRK transfer 20 WCKD "$PUBKEY" "$SPEND_HOOK" "$BULLA" | tee /tmp/xfer.tx | $DRK broadcast
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
        sleep 1
    done
}

propose() {
    MY_ADDR=$($DRK wallet address)
    PROPOSAL="$($DRK dao propose-transfer MiladyMakerDAO 1 5 WCKD "$MY_ADDR" | cut -d' ' -f3)"
    $DRK dao proposal "$PROPOSAL" --mint-proposal > /tmp/propose.tx
    $DRK broadcast < /tmp/propose.tx
}

wait_proposal() {
    PROPOSAL="$($DRK dao proposals MiladyMakerDAO | cut -d' ' -f2)"
    while [ "$($DRK dao proposal $PROPOSAL | grep '^Proposal transaction hash: ' | awk '{print $4}')" = None ]; do
        sleep 1
    done
}

vote() {
    PROPOSAL="$($DRK dao proposals MiladyMakerDAO | cut -d' ' -f2)"
    $DRK dao vote "$PROPOSAL" 1 > /tmp/dao-vote.tx
    $DRK broadcast < /tmp/dao-vote.tx
}

wait_vote() {
    PROPOSAL="$($DRK dao proposals MiladyMakerDAO | cut -d' ' -f2)"
    while [ "$($DRK dao proposal $PROPOSAL | grep '^Current proposal outcome: ' | awk '{print $4}')" != "Approved" ]; do
        sleep 1
    done
}

do_exec() {
    PROPOSAL="$($DRK dao proposals MiladyMakerDAO | cut -d' ' -f2)"
    $DRK dao exec --early $PROPOSAL > /tmp/dao-exec.tx
    $DRK broadcast < /tmp/dao-exec.tx
}

wait_exec() {
    PROPOSAL="$($DRK dao proposals MiladyMakerDAO | cut -d' ' -f2)"
    while [ "$($DRK dao proposal $PROPOSAL | grep '^Proposal was executed on transaction: ' | awk '{print $6}')" = None ]; do
        sleep 1
    done
}

mint_tokens
wait_tokens
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
