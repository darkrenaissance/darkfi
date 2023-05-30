#!/bin/sh -x
set -e

# First run the consensus node:
#
#   ./tmux_sessions.sh now && cd ../../../ && make darkfid && cd -
#   ./clean.sh
#   ./tmux_sessions.sh -v
#   ./sync-wallet.sh
#
# In another term run the wallet syncing:
#
#   ./sync-wallet.sh
#
# Finally you can run this script

mint_tokens() {
    drk token generate-mint
    drk token generate-mint

    TOKEN_ID1=$(drk token list 2>/dev/null | awk 'NR == 3 {print $1}')
    TOKEN_ID2=$(drk token list 2>/dev/null | awk 'NR == 4 {print $1}')

    drk alias add WCKD "$TOKEN_ID1"
    drk alias add MLDY "$TOKEN_ID2"

    ADDR=$(drk wallet --address)

    drk token mint WCKD 42 "$ADDR" > /tmp/mint-wkcd.tx
    drk token mint MLDY 20 "$ADDR" > /tmp/mint-mldy.tx

    drk broadcast < /tmp/mint-wkcd.tx
    drk broadcast < /tmp/mint-mldy.tx

    drk token list
}

token_balance() {
    BALANCE=$(drk wallet --balance 2>/dev/null)
    # No tokens received at all yet
    echo "$BALANCE" | rg "No unspent balances found" > /dev/null
    if [ "$?" = 0 ]; then
        echo 0
        return
    fi
    BALANCE=$(echo "$BALANCE" | rg "$1")
    # Not received yet so no entry
    if [ "$?" = 1 ]; then
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
    drk dao create 20 10 0.67 MLDY > /tmp/dao.dat
    drk dao import MiladyMakerDAO < /tmp/dao.dat
    drk dao list
    drk dao list MiladyMakerDAO

    drk dao mint MiladyMakerDAO > /tmp/dao-mint.tx
    drk broadcast < /tmp/dao-mint.tx
}

wait_dao_mint() {
    while [ "$(drk dao list MiladyMakerDAO | rg "Tx hash" | awk '{print $3}')" = None ]; do
        sleep 1
    done
}

fill_treasury() {
    PUBKEY=$(drk dao list 1 | awk 'NR==9 {print $3}')
    BULLA=$(drk dao list 1 | awk 'NR==4 {print $2}')
    drk transfer 20 WCKD "$PUBKEY" --dao "$BULLA" > /tmp/xfer.tx
    drk broadcast < /tmp/xfer.tx
}

dao_balance() {
    BALANCE=$(drk dao balance 1 2>/dev/null)
    # No tokens received at all yet
    echo "$BALANCE" | rg "No unspent balances found" > /dev/null
    if [ "$?" = 0 ]; then
        echo 0
        return
    fi
    BALANCE=$(echo "$BALANCE" | rg "$1")
    # Not received yet so no entry
    if [ "$?" = 1 ]; then
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
    MY_ADDR=$(drk wallet --address)
    drk dao balance MiladyMakerDAO
    drk dao propose MiladyMakerDAO "$MY_ADDR" 0.1 WCKD > /tmp/propose.tx
    drk broadcast < /tmp/propose.tx
}

proposal_status() {
    PROPOSALS_LEN=$(drk dao proposals MiladyMakerDAO | wc -l)
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
    drk dao vote MiladyMakerDAO "$PROPOSAL_ID" 1 20 > /tmp/dao-vote.tx
    drk broadcast < /tmp/dao-vote.tx
}

vote_status() {
    drk dao proposal MiladyMakerDAO 1 | rg yes > /dev/null
    echo $?
}
wait_vote() {
    while [ "$(vote_status)" != 0 ]; do
        sleep 1
    done
}

do_exec() {
    PROPOSAL_ID=1
    drk dao exec MiladyMakerDAO "$PROPOSAL_ID" > /tmp/dao-exec.tx
    drk broadcast < /tmp/dao-exec.tx
}

drk wallet --keygen
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

