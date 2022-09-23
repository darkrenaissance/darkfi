darkotc
=======

Commandline tool for atomic swaps.


## Usage (localnet)

Prerequisite: Compile all the tools:

```
% make BINS="darkfid drk faucetd darkotc"
```

First start the localnet simulation from `contrib/localnet/darkfid`
called `tmux_sessions.sh` and allow them all to run and sync up.

We will communicate with two darkfid nodes which represent two peers
in the atomic swap, and with the faucet, so we can get some coins in
our wallets.

* The first endpoint (Alice) JSON-RPC URI is: `tcp://127.0.0.1:8440`.
* The second endpoint (Bob) JSON-RPC URI is: `tcp://127.0.0.1:8540`.
* The faucet JSON-RPC URI is: `tcp://127.0.0.1:8640`.


### Airdropping shitcoins

Once the daemons are running, we can interact with them. First we
will airdrop some coins to both Alice and Bob, so they can swap them
between each other. We'll also export some variables for easier usage.

```
% export ALICE_RPC="tcp://127.0.0.1:8440"
% export BOB_RPC="tcp://127.0.0.1:8540"
% export FAUCET_RPC="tcp://127.0.0.1:8640"
% export ALICE_TOKEN="A7f1RKsCUUHrSXA7a9ogmwg8p3bs6F47ggsW826HD4yd"
% export BOB_TOKEN="BNBZ9YprWvEGMYHW4dFvbLuLfHnN9Bs64zuTFQAbw9Dy"
% ./drk -e "${ALICE_RPC}" airdrop --faucet-endpoint "${FAUCET_RPC}" \
    --token-id "${ALICE_TOKEN}" 113
% ./drk -e "${BOB_RPC}" airdrop --faucet-endpoint "${FAUCET_RPC}" \
    --token-id "${BOB_TOKEN}" 14
```

This will airdrop 113 `ALICE_TOKEN` into Alice's wallet, and 14
`BOB_TOKEN` into Bob's wallet.

Wait a bit until the transactions settle on the blockchain and they
appear in your wallet. You can check this with the `drk` utility:

```
% ./drk -e "${ALICE_RPC}" wallet --balance
% ./drk -e "${BOB_RPC}" wallet --balance
```

Once the coins are settled, you should see them in a table after
running one of the above commands.

### Doing everyone's part

After we have the coins in the wallets, Alice and Bob decide they
want to swap 113 `ALICE_TOKEN` for 14 `BOB_TOKEN`. Now both of them
will do a partial transaction which they are going to join if they
both agree on the parameters, and finally publish it on the network.

So first, let's have Alice do her part:

```
% ./darkotc -e "${ALICE_RPC}" init -t "${ALICE_TOKEN}:${BOB_TOKEN}" \
    -v 113:14 > alice_partial_swap
```

This command will make the `darkotc` tool communicate with
her `darkfid` instance and gather necessary information to
create her half of the atomic swap. This includes the ZK mint
and burn proofs, and all the necessary transaction data (see
`darkotc/src/main.rs::PartialSwapData`).

The way this works is that Alice _mints_ the tokens she wants to
receive (in this case, 14 `BOB_TOKEN`, and _burns_ the token she
wants to send to Bob (in this case 113 `ALICE_TOKEN`). In turn,
Bob does the same with his tokens.

Once Alice builds her half of the atomic swap, she can send the newly
created file that we redirected from running the `darkotc` tool to
Bob over an encrypted channel. When Bob has it, he can inspect if
everything is in order, beacuse Alice will send all the blinding
values necessary to convince Bob that the transaction they will make
is true.  Note however that this cannot be exploited in a way of
stealing Alice's coins, because Alice does not create a signature
for the data she is sending, so if Bob published something else,
it would not be considered valid on the network.

To inspect the `alice_partial_swap` file, Bob runs:

```
% ./darkotc -e "${BOB_RPC}" inspect-partial < alice_partial_swap
```

The tool will then verify all pieces of the partial transaction
and report what is or isn't valid. If Bob is satisfied, then he can
proceed by creating his half of the atomic swap:

```
% ./darkotc -e "${BOB_RPC}" init -t "${BOB_TOKEN}:${ALICE_TOKEN}" \
    -v 14:113 > bob_partial_swap
```

(Note how the values separated by `:` are swapped in contrast to what
Alice has done).

Now Bob can send this back to Alice for verification. We'll leave that
as an excercise to the reader.

Once both parties are satisfied with the contents, they can join the
two halves into a full transaction, and sign it. First, Bob will join
both parts and create a signature:

```
% ./darkotc -e "${BOB_RPC}" join alice_partial_swap bob_partial_swap \
    | ./darkotc -e "${BOB_RPC}" sign-tx > bob_signed_swap
```

Now the `bob_signed_swap` file contains a full atomic swap signed
by Bob. However we're still missing Alice's signature. So Bob sends
back the signed data to Alice, and she can now sign and broadcast it
to the network.

```
% ./darkotc -e "${ALICE_RPC}" sign-tx < bob_signed_swap > signed_swap \
    | ./drk -e "${ALICE_RPC}" broadcast
```

On success, Alice should see a transaction ID. Now wait until the
swap settles on the network, and check the balances:

```
% ./drk -e "${ALICE_RPC}" wallet --balance
% ./drk -e "${BOB_RPC}" wallet --balance
```

Alice and Bob successfully executed an atomic swap!
