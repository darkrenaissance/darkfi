DarkFi Node Architecture (DNA)
==============================

The DarkFi ecosystem runs as a network of P2P nodes, where these
nodes interact with each other over specific protocols. Below in
this document, we'll explain how each of the programs fit together
and when combined create a functioning network that becomes DarkFi.

The layers are organized as a bottom-up pyramid, much like the
DarkFi logo:

$$ \setminus validatord / $$
$$  \setminus darkfid  / $$
$$   \setminus drk   / $$

We will start with the top-level daemon - `validatord` - which
serves as the consensus and data storage layer, then we will explain
`darkfid` and its communication with the layer above (`validatord`),
and the layer below (`drk`).

An abstract view of the network looks like the following:

```
[drk] <--> [darkfid] <--> [validatord] <-+
                                         |
[drk] <--> [darkfid] <--> [validatord] <-+
```


## validatord

`validatord` is the DarkFi consensus and data storage layer. Everyone
that runs a validator participates in the network as a data archive,
and is able to store incoming transactions, and relay them to
other validators over the P2P network and protocol. Additionally,
storing this data allows others to replicate it and participate in
the same way.

Provided there is a locked stake on a running validator, the node
can also participate in the Proof-of-Stake consensus, enabling the
ability to vote on incoming transactions rather than just relaying
(and validating) them.

In case the node is not participating in the consensus, it should still
relay incoming transactions to other (ideally consensus-participating)
validators in the network.

### Inner workings

In its database, `validatord` stores transactions and blocks that
have reached consensus. This is commonly known as a _"blockchain"_.
The blockchain is a shared state that is replicated between all
validators in the network.

Additionally, validators keep a pool of incoming transactions
and proposed blocks, which get validated and voted on by the
consensus-participating validators.

The lifetime of an incoming transaction (and block) is as follows:

1. Wait for a transaction
2. Validate incoming transaction (and go back to 1. if invalid)
3. Broadcast transaction to other validators in the network
4. Other validators validate transaction (and go back to 1. if invalid)
5. Leader validates the state transition and proposes a block
6. Consensus-participating nodes validate the state transition and
   vote on the proposed block if the state transition is valid.
7. If the block is confirmed, it is appended to the _blockchain_ and
   is replicated between all validators in the network.


## darkfid

`TODO: Initial sync and retrieving wallet state?`

`darkfid` is the client layer of DarkFi used for wallet management
and transaction broadcasting. The wallet keeps a history of balances,
_coins_, _nullifiers_, and _merkle roots_ that are necessary in order
to create new transactions.

By design, `darkfid` is a light client, since `validatord` stores all
the blockchain data, and `darkfid` can simply query for anything it
is interested in. This allows us to avoid data duplication and simply
utilize our modular architecture. This also means that `darkfid` can
easily be replaced with more specific tooling, if need be.

### Inner workings

Using the P2P network and protocol, `darkfid` can subscribe to
`validatord` in order to receive new _nullifiers_ and _merkle roots_
whenever a new block is confirmed. This allows `darkfid` to update
its local state and enables it to create new valid transactions.

`darkfid` exposes a JSON-RPC endpoint for clients to interact with it.
This allows a number of things, such as: listing balances, creating
and submitting transactions, key management, and more.

When creating a new transactions, `darkfid` uses the local synced state
in order to create new _coins_ and combine them in a transaction. This
transaction is then submitted to the above validator layer where the
transaction will get validated and voted on in order to be included
into a block.


## drk

`drk` is a client tool that interacts with `darkfid` in a user-friendly
way and provides a command-line interface to the DarkFi network and
its functionality.

The interaction with `darkfid` is done over the JSON-RPC protocol
and communicates with the endpoint exposed by `darkfid`.
