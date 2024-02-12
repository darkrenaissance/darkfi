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
| P2P network            | Peer-to-peer network on which nodes communicate with each other                        |
| Finalization           | State achieved when a block and its contents are appended to the canonical blockchain  |
| Fork                   | Chain of block proposals that begins with the last block of the canonical blockchain   |

## Miner main loop

DarkFi uses a Proof of Work RandomX algorithm paired with delayed finality.
Therefore, block production involves the following steps:

* First, a miner grabs its current best ranking fork and extends it with a
  block composed of unproposed transactions from the miner's mempool.

* Then the miner tries to find a nonce such that when the block header is
  hashed its bytes produce a number that is less than the current difficulty
  target of the network, using the [RandomX mining
  algorithm](https://github.com/tevador/RandomX).

* Once the miner finds such a nonce, it broadcasts its block proposal to the
  P2P network. Finally the miner triggers a finalization check to see if its
  newly extended fork can be finalized.

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

Each node listens for new block proposals on the P2P network. Upon receiving
block proposals, nodes try to extend the proposals onto a fork held in memory
(this process is described in the next section). Then nodes trigger a
finalization check to see if their newly extended fork can be finalized.

Upon receiving a new block proposal, miners also check if the extended fork
rank is better than the one they are currently trying to extend. If the fork
rank is better, the miner will stop mining its proposal and start mining the
new best fork.

## Ranking

Block producers create a reward transaction containing a `ECVRF` proof that
contributes to ranking logic. The `VRF` is built using the `pallas::Base` of
the $(n-1)$-block proposal's nonce, the $(n-2)$-block proposal's hash, and the
`pallas::Base` of the block proposal's block height. The `VRF`'s purpose is to
eliminate long range attacks by predicting a high-ranking future block that we
can produce in advance.

Each block proposal is ranked based on the modulus of the $(n-2)$-block
proposal's `VRF` proof (attached to the block producer's reward transaction)
and its `nonce`.

The rank of the genesis block is 0. The rank of the following 2 blocks is equal
to the nonce, since there is no $(n-2)$-block producer or `VRF` attached to the
reward transaction.

For all other blocks, the rank is computed as follows:

1. Grab the `VRF` proof from the reward transaction of the $(n-2)$-block proposal
2. Obtain a big-integer from the big endian output of the `VRF`
3. Compute the rank: `vrf.output` % `nonce` (If `nonce` is 0, rank is equal to `vrf.output`)

To calculate each fork rank, we simply multiply the sum of every block
proposal's rank in the fork by the fork's length. We use the length
multiplier to give a preference to longer forks (i.e. longer forks are
likely to have a higher ranking).

The ranking of a fork is always increasing as new blocks are appended.
To see this, let $F = (M₁ ⋯  Mₙ)$ be a fork with a finite sequence of blocks $(Mᵢ)$
of length $n$. The rank of a fork is calculated as
$$ r_F = n ∑ᵢ₌₁ⁿ \t{rank}(Mᵢ) $$
Let $F' = F ⊕  (Mₙ₊₁)$ of length $n + 1$ be the fork created by appending
the block $Mₙ₊₁$ to $F$. Then we see that
$$ r_{F'} > r_F $$
since $\t{rank}(M) > 0$ for all $M$.

## Fork extension

Since there can be more than one block producer, each node holds a set of known
forks in memory. Nodes extend the best ranking fork in memory when producing a
block.

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
| [C]           | Canonical (finalized) blockchain block |
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

If more than one fork exists with same rank, the node will not finalize any
block proposals. If the fork's length exceeds the security threshold, the node
will finalize all block proposals, excluding the last ($n$)-block proposal, by
appending them to the canonical blockchain. We exclude the last ($n$)-block
proposal to eliminate network race conditions for blocks of the same height.

Once finalized, all the remaining fork chains are removed from the node's
memory pool.

Because of this design, finalization cannot occur while there are competing
fork chains of the same rank whose length exceeds the security threshold. In
this case, finalization will occur when a single highest ranking fork emerges.

We continue Case 3 from the previous section to visualize this logic.

The finalization threshold used in this example is 3 blocks. A node observes 2
proposals. One extends the F0 fork and the other extends the F2 fork:

                   |--[M0]--[M2]+--[M5] <-- F0
    [C]--...--[C]--|
                   |--[M1]              <-- F1
                   |
                   |--[M0]--[M3]+--[M6] <-- F2
                   |
                   |--[M4]              <-- F3

The two competing fork chains also have the same rank, therefore finalization
cannot occur.

Later, the node only observes 1 proposal, extending the F2 fork:

                   |--[M0]--[M2]--[M5]        <-- F0
    [C]--...--[C]--|
                   |--[M1]                    <-- F1
                   |
                   |--[M0]--[M3]--[M6]+--[M7] <-- F2
                   |
                   |--[M4]                    <-- F3

When the finalization sync period starts, the node finalizes fork F2 and all
other forks get dropped:

                   |/--[M0]--[M2]--[M5]      <-- F0
    [C]--...--[C]--|
                   |/--[M1]                  <-- F1
                   |
                   |--[M0]--[M3]--[M6]--[M7] <-- F2
                   |
                   |/--[M4]                  <-- F3

The canonical blockchain now contains blocks M0, M3, M6 from fork F2. The
current state is:

    [C]--...--[C]--|--[M7] <-- F2

# Appendix

This section gives further details about the high level structures that will be
used by the protocol.

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

