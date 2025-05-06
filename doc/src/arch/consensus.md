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
| Confirmation           | State achieved when a block and its contents are appended to the canonical blockchain  |
| Fork                   | Chain of block proposals that begins with the last block of the canonical blockchain   |
| MAX_INT                | The maximum 32 bytes (256 bits) integer 2^256 − 1                                      |

## Miner main loop

DarkFi uses RandomX Proof of Work algorithm.
Therefore, block production involves the following steps:

* First, a miner grabs its current best ranking fork and extends it with a
  block composed of unproposed transactions from the miner's mempool.

* Then the miner tries to find a nonce such that when the block header is
  hashed its bytes produce a number that is less than the current difficulty
  target of the network, using the [RandomX mining
  algorithm](https://github.com/tevador/RandomX).

* Once the miner finds such a nonce, it broadcasts its block proposal to the
  P2P network. Finally the miner triggers a confirmation check to see if its
  newly extended fork can be confirmed.

Pseudocode:
```
loop {
    fork = find_best_fork()

    block = generate_next_block(fork)

    mine_block(block)

    p2p.broadcast_proposal(block)

    fork.append_proposal(block)

    chain_confirmation()
}
```

## Listening for block proposals

Each node listens for new block proposals on the P2P network. Upon receiving
block proposals, nodes try to extend the proposals onto a fork held in memory
(this process is described in the next section). Then nodes trigger a
confirmation check to see if their newly extended fork can be confirmed.

Upon receiving a new block proposal, miners also check if the extended fork
rank is better than the one they are currently trying to extend. If the fork
rank is better, the miner will stop mining its proposal and start mining the
new best fork.

## Ranking

Each block proposal is ranked based on how hard it is to produce. To measure
that, we compute the squared distance of its height target from `MAX_INT`.
For two honest nodes that mine the next block height of the highest ranking
fork, their block will have the same rank. To mitigate this tie scenario,
we also compute the squared distance of the blocks `RandomX` hash from
`MAX_INT`, allowing us to always choose the actual higher ranking block for
that height, in case of ties. The complete block rank is a tuple containing
both squared distances.

Proof of Work algorithm lowers the difficulty target as hashpower grows.
This means that blocks will have to be mined for a lower target, therefore
rank higher, as they go further away from `MAX_INT`.

Similar to blocks, blockchain/forks rank is a tuple, with the first part being the
sum of its block's squared target distances, and the second being the sum of
their squared hash distances. Squared distances are used to disproportionately
favors smaller targets, with the idea being that it will be harder to trigger
a longer reorg between forks. When we compare forks, we first check the first
sum, and if it's tied, we use the second as the tie breaker, since we know it
will be statistically unique for each sequence.

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

| Description                                   | Handling                                                            |
|-----------------------------------------------|---------------------------------------------------------------------|
| Block extends a known fork at its end         | Append block to fork                                                |
| Block extends a known fork not at its end     | Create a new fork up to the extended block and append the new block |
| Block extends canonical blockchain at its end | Create a new fork containing the new block                          |
| Block doesn't extend any known chain          | Check if a reorg should be executed                                 |

### Visual Examples

| Symbol        | Description                            |
|---------------|----------------------------------------|
| [C]           | Canonical (confirmed) blockchain block |
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

##### Case 4

Reorg happened and we rebuild the chain:

                              |--[M0]--[M2] <-- F0
    [C]--...--[C]/--...--[C]--|
               |              |--[M1]       <-- F1
               |              |
               |              |--[M0]--[M3] <-- F2
               |              |
               |              |--[M4]       <-- F3
               |
               ...--...--[C]--|+--[M5]      <-- F4


## Confirmation

Based on the rank properties, each node will diverge to the highest ranking
fork, and new fork will emerge extending that at its tips.
A security threshold is set, which refers to the height where the probability
to produce a fork, able to reorg the current best ranking fork reaches zero,
similar to the # of block confirmation used by other PoW based protocols.

When the confirmation check kicks in, each node will grab its best fork.
If the fork's length exceeds the security threshold, the node will push (confirm)
its first proposal to the canonical blockchain. The fork acts as a queue (buffer)
for the to-be-confirmed proposals.

Once a confirmation occurs, all the fork chains not starting with the confirmed
block(s) are removed from the node's memory pool.

We continue Case 3 from the previous section to visualize this logic.

The confirmation threshold used in this example is 3 blocks. A node observes 2
proposals. One extends the F0 fork and the other extends the F2 fork:

                   |--[M0]--[M2]+--[M5] <-- F0
    [C]--...--[C]--|
                   |--[M1]              <-- F1
                   |
                   |--[M0]--[M3]+--[M6] <-- F2
                   |
                   |--[M4]              <-- F3


Later, the node only observes 1 proposal, extending the F2 fork:

                   |--[M0]--[M2]--[M5]        <-- F0
    [C]--...--[C]--|
                   |--[M1]                    <-- F1
                   |
                   |--[M0]--[M3]--[M6]+--[M7] <-- F2
                   |
                   |--[M4]                    <-- F3

When the confirmation sync period starts, the node confirms block M0 and
keeps the forks that extend that:

                   |--[M0]--[M2]--[M5]       <-- F0
    [C]--...--[C]--|
                   |/--[M1]                  <-- F1
                   |
                   |--[M0]--[M3]--[M6]--[M7] <-- F2
                   |
                   |/--[M4]                  <-- F3

The canonical blockchain now contains blocks M0 and the current state is:

                   |--[M2]--[M5]       <-- F0
    [C]--...--[C]--|
                   |--[M3]--[M6]--[M7] <-- F2

# Appendix: Data Structures

This section gives further details about the high level structures that will be
used by the protocol.

Note that for hashes, we define custom types like `TransactionHash`, but here
we will just use the raw byte representation `[u8; 32]`.

| Index         | Type  | Description                               |
|---------------|-------|-------------------------------------------|
| `block_index` | `u32` | Block height                              |
| `tx_index`    | `u16` | Index of a tx within a block              |
| `call_index`  | `u8`  | Index of contract call within a single tx |

`u32` can store 4.29 billion blocks, which with a 90 second blocktime
corresponds to 12.2k years.

`u16` max value 65535 which is far above the expected limits. By comparison
the tx in Bitcoin with the most outputs has 2501.

## Header

| Field               | Type       | Description                                              |
|---------------------|------------|----------------------------------------------------------|
| `version`           | `u8`       | Block version                                            |
| `previous`          | `[u8; 32]` | Previous block hash                                      |
| `height`            | `u32`      | Block height                                             |
| `timestamp`         | `u64`      | Block creation timestamp                                 |
| `nonce`             | `u64`      | The block's nonce value                                  |
| `transactions_root` | `[u8; 32]` | Merkle tree root of the block's transactions hashes      |
| `state_root`        | `[u8; 32]` | Contracts states Monotree(SMT) root the block commits to |

## Block

| Field       | Type           | Description              |
|-------------|----------------|--------------------------|
| `header`    | `[u8; 32]`     | Block header hash        |
| `txs`       | `Vec<[u8; 32]` | Transaction hashes       |
| `signature` | `Signature`    | Block producer signature |

## Blockchain

| Field    | Type         | Description                                |
|----------|--------------|--------------------------------------------|
| `blocks` | `Vec<Block>` | Series of blocks consisting the Blockchain |
| `module` | `PoWModule`  | Blocks difficulties state used by RandomX  |

## Fork

| Field       | Type           | Description                      |
|-------------|----------------|----------------------------------|
| `chain`     | `Blockchain`   | Forks current blockchain state   |
| `proposals` | `Vec<[u8; 32]` | Fork proposal hashes sequence    |
| `mempool`   | `Vec<[u8; 32]` | Valid pending transaction hashes |

## Validator

| Field       | Type              | Description                            |
|-------------|-------------------|----------------------------------------|
| `canonical` | `Blockchain`      | Canonical (confirmed) blockchain       |
| `forks`     | `Vec<Blockchain>` | Fork chains containing block proposals |

# Appendix: Ranking Blocks

## Sequences

Denote blocks by the symbols $bᵢ ∈ B$, then a sequence of blocks (alternatively
a fork) is an ordered series $𝐛 = (b₁, …, bₘ)$.

Use $S$ for all sets of sequences for blocks in $B$.

## Properties for Rank

Each block is associated with a target $T : B → 𝕀$ where $𝕀 ⊂ ℕ$.

1. Blocks with lower targets are harder to create and ranked higher in a sequence of blocks.
2. Given two competing forks $𝐚 = (a₁, …, aₘ)$ and $b = (b₁, …, bₙ)$,
   we wish to select a winner. Assume $𝐚$ is the winner, then $∑ T(aᵢ) ≤ ∑ T(bᵢ)$.
3. There should only ever be a single winner.
   When $∑ T(aᵢ) = ∑ T(bᵢ)$, then we have logic to break the tie.

Property (2) can also be statistically true for $p > 0.5$.

This is used to define a *fork-ranking* function $W : S → ℕ$.
This function must *always* have unique values for distinct sequences.

### Additivity

We also would like the property $W$ is additive on subsequences
$$ W((b₁, …, bₘ)) = W((b₁)) + ⋯ + W((bₘ)) $$
which allows comparing forks from any point within the blockchain. For example
let $𝐬 = (s₁, …, sₖ)$ be the blockchain together with forks $𝐚, 𝐛$ extending $𝐬$
into $𝐬 ⊕  𝐚 = (s₁, …, sₖ, a₁, …, aₘ)$ and $𝐬 ⊕  𝐛 = (s₁, …, sₖ, b₁, …, bₙ)$.
Then we have that
$$ W(𝐬 ⊕  𝐚) < W(𝐬 ⊕  𝐛) ⟺  W(𝐚) < W(𝐛) $$
which means it's sufficient to compare $𝐚$ and $𝐛$ directly.

## Proposed Rank

With a PoW mining system, we are guaranteed to always have the block hash
$h(b) ≤ T(b)$. Since the block hashes $( h(b₁), …, h(bₘ) )$ for a sequence
$( b₁, …, bₘ )$ have the property that $∑ h(bᵢ) ≤ ∑ T(bᵢ)$, as well as being
sufficiently random, we can use them to define our work function.

Because $W$ is required to be additive, we define a block work function
$w : B → ℕ$, and $W(𝐛) = ∑ w(bᵢ)$.

The block work function should have a statistically higher score for
blocks with a smaller target, and always be distinct for unique blocks.
We define $w$ as
$$ w(b) = \max(𝕀) - h(b) $$
since $h(b) < T(b) < \max(𝕀)$ this function is well defined on the codomain.

## Hash Function

Let $𝕀$ be a fixed subset of $ℕ$ representing the output of a hash function
$[0, \max(𝕀)]$.

**Definition:** a *hash function* is a function $H : ℕ → 𝕀$ having the
following properties:

1. *Uniformity*, for any $y ∈ 𝕀$ and any $n ∈ ℕ$, there exists an $N > n$
   such that $H(N) = y$.
2. *One-way*, for any $y ∈ 𝕀$, we are unable to construct an $x ∈ ℕ$ such
   that $H(x) = y$.

Note: the above notions rely on purely algebraic properties of $H$ without
requiring the machinery of probability. The second property of being one-way
is a stronger notion than $\ran(H)$ being statistically random. Indeed if the
probability is non-zero then we could find such an $(x, y)$ which breaks the
one-way property.

**Theorem:** *given a hash function $H : ℕ → 𝕀$ as defined above, it's impossible to
construct two distinct sequences $𝐚 = (a₁, …, aₘ)$ and $𝐛 = (b₁, …, bₙ)$
such that $H(a₁) + ⋯ + H(aₘ) = H(b₁) + ⋯ + H(bₙ)$.*

By property (2), we cannot find a $H(x) = 0$.
Again by (2), we cannot construct an $x$ such that $H(x) + H(a) = H(b)$ for
any $a, b ∈ ℕ$. Recursive application of (2) leads us to the stated theorem.

