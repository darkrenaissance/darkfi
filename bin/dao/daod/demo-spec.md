---
title: DAO demo architecture
author: jstark
---

This document outlines a simple demo to showcase the smart contract
schema underlying the initial DAO MVP. We have tried to mimic the
basic DarkFi architecture while remaining as simple as possible.

We do not have a blockchain, p2p network, or encrypted wallet database
in this highly simplified demo. It is just a local network of 4 nodes
and a relayer.  The values are all stored in memory.

# Layers

**bin**
Located in darkfi/bin/dao.

* **daod/**: receives rpc requests and operates a `client`.
* **dao-cli/**: command-line interface that receives input and sends rpc requests.
* **relayerd/**: receives transactions on TCP and relays them to all nodes.

**src**
Located in darkfi/bin/dao/daod.

* **contract/**: source code for dao and money contracts.
* **util/**: demo-wide utilities.
* **state/**: stores dao and money states.
* **tx**: underlying types required by transactions, function calls and call data.
* **node/**: a dao full node.

**node**
A dao node containing `client` and `wallet` submodules.

* **client/**: operates a wallet and performs state transition and validate methods.
* **wallet/**: owns and operates secret values.

**proof**
Located in darkfi/bin/dao/proof. Contains the zk proofs.

# Command Flow

The following assumes that a user has already compiled the zk contracts
by running `make`.

This requires a `Makescript` as follows:

```
ZK_SRC_FILES := $(wildcard proof/*.zk)
ZK_BIN_FILES := $(patsubst proof/%.zk, proof/%.zk.bin, $(ZK_SRC_FILES))

daod: $(ZK_BIN_FILES)
	cargo run --release

proof/%.zk.bin: proof/%.zk
	zkas $<
```

We will also need to write a shell script that opens 9 terminals and
runs the following:

*Terminal 1:* relayerd.
*Terminal 2-5:* 4 instances of daod.
*Terminal 6-9:* 4 instances of dao-cli.

Relayerd and daod should be sent to the background, so the demo will
consist visually of 4 terminals running dao-cli.

## Start relayer

1. `relayerd` starts a listener for all TCP ports specified in config.

## Initialize DAO 

Note: this happens automatically on daod first run.

1. `daod`: starts a listener on the relayer TCP port.
2. `daod:` creates a client and calls `client.init()`.
3. `client`: creates a money wallet.
4. `money-wallet`: generates cashier, faucet keys.
5. `client`: gets public keys from wallet and calls `state.new()`.
6. `state`: creates ZkContractTable, StateRegistry. 
7. `state`: loads all zk binaries and saves them in ZkContractTable.
8. `state`: creates a new money/dao state and registers them in StateRegistry.

## Stage 1: create DAO

1. `dao-cli:` sends `create()` rpc request to daod.
2. `daod`: receives rpc request and calls `client.create()`.
3. `client`: creates a dao wallet.
4. `dao-wallet`: specifies the dao params.
5. `dao-wallet`: creates a dao keypair, bulla blind, and signature secret.
 
**build sequence.**

Note: Builders differ according to the FuncCall, but the basic sequence
is the same.

6. `dao-wallet`: build: creates dao_contract::mint::wallet::Builder.
7. `dao-wallet`: generates a FuncCall from builder.build().
8. `dao-wallet`: adds FuncCall to a vector.
9. `dao-wallet`: sign the vector of FuncCalls.

**send sequence.**

10. `dao-wallet`: create a Transaction.
11. `dao-wallet`: send the Transaction to the relayer.
12. `relayer`: receives a Transaction on one of its connections.
13. `relayer`: relays the Transaction to all connected nodes.

**recv sequence.**

14. `daod`: receives a Transaction on its relayerd listener.
15. 'daod`: sends the Transaction to Client.

**validate sequence.**

16. `client`: validate: creates an empty vector of updates.
16. `client`: loops through all FuncCalls in the Transaction.
17. `client`: runs a match statement on the FUNC_ID.
18. `client`: finds mint FUNC_ID and runs a state transition function.
20. `client`: pushes the result to Vec<Update>
21. `client`: outside the loop, atomically applies all updates.
22. `client`: calls zk_verify() on the Transaction.
23. `client`: verifies signatures.

------------------------------------------------------------------------

24. `client`: sends Transaction to the relayer.
25. `relayer`: receives Transaction and relays.
* TODO: `dao-wallet`: waits until Transction is confirmed. (how?)
27. `dao-wallet`: look up the dao state and call witness().
28. `dao-wallet`: get the dao bulla from the Transaction.
29. `dao-cli`: print "Created DAO {}".

## Stage 2: fund DAO

* TODO: for the demo it might be better to call mint() first and then
fund(), passing the values into fund()

Here we are creating a treasury token and sending it to the DAO.

1. `dao-cli:` `fund()` rpc request to daod
2. `daod`: receives rpc request and calls `client.fund()`.
3. `client`: creates treasury token, random token ID and supply

Note: dao-wallet must manually track coins to retrieve coins belonging
to its private key.

4. `dao-wallet`: looks up the money state, and calls state.wallet_cache.track()

5. `money-wallet`: sets spend hook to dao_contract::exec::FUNC_ID
5. `money-wallet`: sets user_data to dao_bulla

* TODO: how does it get the dao_bulla? Must be stored somewhere.

6. `money-wallet`: specifies dao public key and treasury token BuilderOutputInfo.
5. `money-wallet`: runs the build sequence for money::transfer.
9. `money-wallet`: create Transaction and send.
10. `relayer`: receives Transaction and relays.
11. `daod`: receives a Transaction and sends to client.
12. `client`: runs the validate sequence.

Note: here we get all coins associated with the private key.
13. `dao-wallet`: looks up the state and calls WalletCache.get_received()
14. `dao-wallet`: check the coin is valid by recreating Coin
15. `daod`: sendswith token ID and balance to dao-cli.
16. `dao-cli`: displays data using pretty table.

## Stage 3: airdrop

1. `dao-cli`: calls keygen()
2. `daod`: client.keygen()
3. `daod`: money-wallet.keygen()
4. `money-wallet`: creates new keypair
5. `money-wallet`: looks up the money_contract State and calls WalletCache.track()
6. `money-wallet`: return the public key
7. `dao-cli`: prints the public key

Note: do this 3 times to generate 3 pubkey keys for different daod instances.

8. `dao-cli`: calls mint()
9. `daod`: call client.mint()
10. `client:` creates governance token with random token ID and supply
11. `dao-cli`: prints "created token {} with supply {}"
12. `dao-cli`: calls airdrop() and passes a value and a pubkey.
13. `dao-wallet:` runs the build sequence for money::transfer.
14. `dao-wallet`: create Transaction and send.
15. `relayer`: receives Transaction and relays.
16. `daod`: receives a Transaction and sends to client.
17. `client`: runs the validate sequence.
18. `money-wallet`: state.wallet_cache.get_received()
19. `money-wallet`: check the coin is valid by recreating Coin
20. `daod`: sends token ID and balance to cli
21. `dao-cli`: prints "received coin {} with value {}".

* TODO: money-wallet must keep track of Coins and have a flag for whether or not they are spent.
* Hashmap of <Coin, bool> ?

## Stage 4: create proposal

* TODO: maybe for the demo we should just hardcode a user/ proposal recipient.

1. `dao-cli`: calls propose() and enter a user pubkey and an amount
3. `dao-wallet`: runs the build sequence for dao_contract::propose
4. `dao-wallet`: specifies user pubkey, amount and token ID in Proposal
5. `dao-cli`: prints "Created proposal to send {} xDRK to {}"
6. `dao-wallet`: create Transaction and send.
7. `relayer`: receives Transaction and relays.
8. `daod`: receives a Transaction and sends to client.
9. `client`: runs the validate sequence.
* TODO: how does everyone have access to DAO private key?
10. `dao-wallet`: reads received proposal and tries to decrypt Note
11. `dao-wallet`: sends decrypted values to daod
12. `dao-cli`: prints "Proposal is now active"

## Stage 5 vote

1. `dao-cli`: calls vote() and enters a vote option (yes or no) and an amount
2. `daod`: calls client.vote()
3. `money-wallet`: get money_leaf_position and money_merkle_path
4. `money-wallet`: create builder sequence for dao_contract::vote
5. `money-wallet`: specify dao_keypair in vote_keypair field
* TODO: this implies that money-wallet is able to access private values in dao-wallet
6. `money-wallet`: signs and sends
7. `relayer`: receives Transaction and relays.
8. `daod`: receives a Transaction and sends to client.
9. `client`: runs the validate sequence.
10. `dao-wallet`: tries to decrypt the Vote.
11. `dao-cli`: prints "Received vote {} value {}"

Note: repeat 3 times with different values and vote options.

* TODO: ignore section re: vote commitments?
* TODO: determine outcome: yes_votes_value/ all_votes_value
        e.g. when the quorum is reached, print "Quorum reached! Outcome {}"
        or just hardcode it for X n. of voters

## Stage 6: Executing the proposal

1. `dao-cli`: calls exec()
* TODO: how does dao have access to user data?
2. `dao-wallet`: get money_leaf_position and money_merkle_path
3. `dao-wallet`: specifies user_kaypair and proposal amount in 1st output
4. `dao-wallet`: specifies change in 2nd output
5. `dao-wallet`: run build sequence for money_contract::transfer
6. `dao-wallet`: run build sequence for dao_contract::exec
7. `dao-wallet`: signs transaction and sends
8. `relayer`: receives Transaction and relays.
9. `daod`: receives a Transaction and sends to client.
10. `client`: runs the validate sequence.
