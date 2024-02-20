#!/bin/sh
set -e
set -x

# Path to `drk` binary
DRK="../../../drk -c drk.toml"

# First run the consensus node and the faucet:
#
#   ./clean.sh
#   ./tmux_sessions.sh -vv
#
# In another term run the wallet syncing:
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

    ADDR="$($DRK wallet --address)"

    $DRK token mint WCKD 42 "$ADDR" | tee /tmp/mint-wkcd.tx | $DRK broadcast
    $DRK token mint MLDY 20 "$ADDR" | tee /tmp/mint-mldy.tx | $DRK broadcast

    $DRK token list
}

token_balance() {
    BALANCE="$($DRK wallet --balance 2>/dev/null)"

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
    $DRK dao create 20 10 0.67 MLDY > /tmp/dao.dat
    $DRK dao import MiladyMakerDAO < /tmp/dao.dat
    $DRK dao list
    $DRK dao list MiladyMakerDAO

    $DRK dao mint MiladyMakerDAO | tee /tmp/dao-mint.tx | $DRK broadcast
}

wait_dao_mint() {
    while [ "$($DRK dao list MiladyMakerDAO | grep '^Tx hash: ' | awk '{print $3}')" = None ]; do
        sleep 1
    done
}

fill_treasury() {
    PUBKEY="$($DRK dao list 1 | grep '^Public key: ' | cut -d ' ' -f3)"
    BULLA="$($DRK dao list 1 | grep '^Bulla: ' | cut -d' ' -f2)"
    $DRK transfer 20 WCKD "$PUBKEY" --dao "$BULLA" | tee /tmp/xfer.tx | $DRK broadcast
}

dao_balance() {
    BALANCE=$($DRK dao balance 1 2>/dev/null)
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
    MY_ADDR=$($DRK wallet --address)
    $DRK dao balance MiladyMakerDAO
    $DRK dao propose MiladyMakerDAO "$MY_ADDR" 0.1 WCKD > /tmp/propose.tx
    $DRK broadcast < /tmp/propose.tx
}

proposal_status() {
    PROPOSALS_LEN=$($DRK dao proposals MiladyMakerDAO | wc -l)
    if [ "$PROPOSALS_LEN" = 0 ]; then
        echo 0
    else
        echo 1
    fi
}

wait_proposal() {
    while [ "$(proposal_status)" = 0 ]; do
        sleep 1
    done
}

vote() {
    PROPOSAL_ID=1
    $DRK dao vote MiladyMakerDAO "$PROPOSAL_ID" 1 20 > /tmp/dao-vote.tx
    $DRK broadcast < /tmp/dao-vote.tx
}

vote_status() {
	PROPOSAL_ID=1
    $DRK dao proposal MiladyMakerDAO "$PROPOSAL_ID" | grep -q yes
    echo $?
}

wait_vote() {
    while [ "$(vote_status)" != 0 ]; do
        sleep 1
    done
}

do_exec() {
    PROPOSAL_ID=1
    $DRK dao exec MiladyMakerDAO "$PROPOSAL_ID" > /tmp/dao-exec.tx
    $DRK broadcast < /tmp/dao-exec.tx
}

$DRK wallet --keygen
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
