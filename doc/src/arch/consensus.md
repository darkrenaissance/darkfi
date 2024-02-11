# Consensus

This section of the book describes how nodes participating in the DarkFi
blockchain achieve consensus.

## Glossary

| Name                   | Description                                                                            |
|------------------------|----------------------------------------------------------------------------------------|
| Consensus              | Algorithm for reaching blockchain consensus between participating nodes                |
| Node/Validator         | DarkFi daemon participating in the network                                             |
| Miner                  | Block producer                                                                         |
| Unproposed Transaction | Transaction that exists in the memory pool but has not yet been included in a proposal |
| Block proposal         | Block that has not yet been appended onto the canonical blockchain                     |
| P2P network            | Peer-to-peer network on which Nodes communicate with each other                        |
| Finalization           | State achieved when a block and its contents are appended to the canonical blockchain  |
| Fork                   | Chain of block proposals that begins with the last block of the canonical blockchain   |

## Miner main loop

DarkFi is using Proof of Work RandomX algorithm paired with Delayed finality.
Therefore, block production involves the following steps:

Miner grabs its current best ranking fork and composes a valid block using
unproposed transactions from their mempool, extending it.
Then they try to find a nonce that makes the blocks header hash bytes produce
a number that is less than the current difficulty target of the network,
using the [RandomX mining algorithm](https://github.com/tevador/RandomX).
Once they find such a nonce, they can propose(broadcast their block proposal
to the P2P network. After that, they trigger their finalization check, to
see if their newlly extended fork can be finalized.

Pseudocode:
```
loop {
    fork = find_best_fork()

    block = generate_next_block(fork)

    mine_block(block)

    p2p.broadcast_proposal(block)

    fork.append_proposal(block)

    chain_finalization()
}
```

## Listening for block proposals

Each node listens to new block proposals from the network.
Upon receiving block proposals, they try to extend the proposals
onto a fork that they hold in memory. This process is described in the
next section. After that, they trigger their finalization check, to
see if their newlly extended fork can be finalized.
Miner, upon receiving a new block proposal, will also check if the
extended fork rank is better than the one they currently try to extend,
in order to stop mining its proposal and start mining the new best fork.

## Ranking

Miners attaches an `ECVRF` proof in their reward transaction, which is then
used as part of our ranking logic. This `VRF` is builded using the `pallas::Base`
of the previous proposal nonce, the previous proposa previous proposal hash, and
the `pallas::Base` of the proposals block height. This `VRF` helps us eliminate
long range attacks, aka predicting a future high ranked block we can produce in advance.

Each proposed block has a ranked based on the modulus of its previous proposal
previous proposal `VRF` proof and its `nonce`. Genesis block has rank 0.
First 2 blocks rank is equal to their nonce, since their previous previous block
producer doesn't exist, or have a `VRF` attached to their reward transaction.
For rest blocks, the rank computes as following:
1. Grab the `VRF` proof from the reward transaction of the previous previous proposal
2. Generate a `pallas::Base` from the `blake3::Hash` bytes of the proof
3. Generate a `u64` using the first 8 bytes from the `pallas::Base` of the proofs hash
4. Compute the rank: `vrf_u64` % `nonce` (If `nonce` is 0, rank is equal to `vrf_u64`)

To calculate each fork rank, we simply sum all its block proposals ranks and multiply
that with the forks length. We use the length multiplier to give a chance of higher
ranking to longer forks.

## Fork extension

Since there can be more than one block producers, each node holds a set of
known forks in memory. When a node produces a block, they extend the best
ranking fork they hold.

Upon receiving a block, one of the following cases may occur:

| Description                               | Handling                                                            |
|-------------------------------------------|---------------------------------------------------------------------|
| Block extends a known fork at its end     | Append block to fork                                                |
| Block extends a known fork not at its end | Create a new fork up to the extended block and append the new block |
| Block extends canonical blockchain        | Create a new fork containing the new block                          |
| Block doesn't extend any known chain      | Ignore block                                                        |

### Visual Examples

| Symbol        | Description                            |
|---------------|----------------------------------------|
| [C]           | Canonical(finalized) blockchain block  |
| [C]--...--[C] | Sequence of canonical blocks           |
| [Mn]          | Proposal produced by Miner n           |
| Fn            | Fork name to identify them in examples |
| +--           | Appending a block to fork              |
| /--           | Dropped fork                           |

Starting state:

                   |--[M0] <-- F0
    [C]--...--[C]--|
                   |--[M1] <-- F1

Blocks on same Y axis have the same height.

#### Case 1

Extending F0 fork with a new block proposal:

                   |--[M0]+--[M2] <-- F0
    [C]--...--[C]--|
                   |--[M1]        <-- F1

#### Case 2

Extending F0 fork at [M0] block with a new block proposal, creating a new fork chain:

                   |--[M0]--[M2]   <-- F0
    [C]--...--[C]--|
                   |--[M1]         <-- F1
                   |
                   |+--[M0]+--[M3] <-- F2

##### Case 3

Extending the canonical blockchain with a new block proposal:

                   |--[M0]--[M2] <-- F0
    [C]--...--[C]--|
                   |--[M1]       <-- F1
                   |
                   |--[M0]--[M3] <-- F2
                   |
                   |+--[M4]      <-- F3


## Finalization

When the finalization check kicks in, each node will grab its best fork.
If more than one forks exist with same rank, node doesn't finalize any of them.
If the fork has reached greater length than the security threshold, node
finalizes all block proposals, excluding the last one, by appending them to the
canonical blockchain. We exclude the last proposal, to eliminate network race
conditions for same height blocks.

Once finalized, all rest fork chains are removed from the nodes memory pool.
Practically this means that no finalization can occur while there are
competing fork chains of the same rank, over the security threshold.
In such a case, finalization will occur when we have a single highest ranking fork.

We continue Case 3 from the previous section to visualize this logic.
The finalization threshold used in the example is 3 blocks.
A node observes 2 proposals. One extends the F0 fork, and the other
extends the F2 fork:

                   |--[M0]--[M2]+--[M5] <-- F0
    [C]--...--[C]--|
                   |--[M1]              <-- F1
                   |
                   |--[M0]--[M3]+--[M6] <-- F2
                   |
                   |--[M4]              <-- F3

The two competing fork chains managed to also have the same rank,
therefore finalization cannot occur.

Later, the node only observes 1 proposal, extending the F2 fork:

                   |--[M0]--[M2]--[M5]        <-- F0
    [C]--...--[C]--|
                   |--[M1]                    <-- F1
                   |
                   |--[M0]--[M3]--[M6]+--[M7] <-- F2
                   |
                   |--[M4]                    <-- F3

When the finalization sync period starts, the node finalizes fork
F2 and all other forks get dropped:

                   |/--[M0]--[M2]--[M5]      <-- F0
    [C]--...--[C]--|
                   |/--[M1]                  <-- F1
                   |
                   |--[M0]--[M3]--[M6]--[M7] <-- F2
                   |
                   |/--[M4]                  <-- F3

The canonical blockchain now contains blocks M0, M3, M6 from fork F2,
and the current state is:

    [C]--...--[C]--|--[M7] <-- F2

# Appendix

This section gives further details about the high level structures that will
be used by the protocol.

## Header

| Field       | Type           | Description                                    |
|-------------|----------------|------------------------------------------------|
| `version`   | `u8`           | Block version                                  |
| `previous`  | `blake3::Hash` | Previous block hash                            |
| `epoch`     | `u64`          | Epoch number                                   |
| `height`    | `u64`          | Block height                                   |
| `timestamp` | `Timestamp`    | Block creation timestamp                       |
| `nonce`     | `u64`          | The block's nonce value                        |
| `tree`      | `MerkleTree`   | Merkle tree of the block's transactions hashes |

## Block

| Field       | Type                | Description              |
|-------------|---------------------|--------------------------|
| `header`    | `blake3::Hash`      | Block header hash        |
| `txs`       | `Vec<blake3::Hash>` | Transaction hashes       |
| `signature` | `Signature`         | Block producer signature |

## Blockchain

| Field    | Type         | Description                                |
|----------|--------------|--------------------------------------------|
| `blocks` | `Vec<Block>` | Series of blocks consisting the Blockchain |
| `module` | `PoWModule`  | Blocks difficulties state used by RandomX  |

## Fork

| Field       | Type                | Description                      |
|-------------|---------------------|----------------------------------|
| `chain`     | `Blockchain`        | Forks current blockchain state   |
| `proposals` | `Vec<blake3::Hash>` | Fork proposal hashes sequence    |
| `mempool`   | `Vec<blake3::Hash>` | Valid pending transaction hashes |

## Validator

| Field       | Type              | Description                            |
|-------------|-------------------|----------------------------------------|
| `canonical` | `Blockchain`      | Canonical (finalized) blockchain       |
| `forks`     | `Vec<Blockchain>` | Fork chains containing block proposals |

