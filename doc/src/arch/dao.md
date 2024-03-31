# DAO Short Explainer

## Prerequisites

There is a scheme called commit-and-reveal, where given an object $x$
(or set of objects), you generate a random blind $b$, and construct the
commitment $C = \textrm{hash}(x, b)$. By publishing $C$, you are
commmitted to the value $x$.

Secondly, we may wish to publish long lived objects on the blockchain,
which are essentially commitments to several parameters that represent
an object defining its behaviour. In lieu of a better term, we call
these bullas.

## `DAO::mint()`: Establishing the DAO

From `darkfi/src/contract/dao/proof/dao-mint.zk`:
```
	bulla = poseidon_hash(
		dao_proposer_limit,
		dao_quorum,
		dao_approval_ratio_quot,
		dao_approval_ratio_base,
		gov_token_id,
		dao_public_x,
		dao_public_y,
		dao_bulla_blind,
	);
```

Brief description of the DAO bulla params:

* **proposer_limit**: minimum deposit required for proposals to become valid.
  TODO: rename to `min_deposit`.
* **quorum**: minimum threshold of votes before it's allowed to pass.
  Normally this is implemented as min % of voting power, but we do this in
  absolute value
* **approval_ratio**: proportion of winners to losers for a proposal to pass.

Currently there is no notion of veto although it could be trivially added
if desired.

## `DAO::propose()`: Propose the Vote

From `darkfi/src/contract/dao/proof/dao-propose-main.zk`:
```
	proposal_bulla = poseidon_hash(
		proposal_dest_x,
		proposal_dest_y,
		proposal_amount,
		proposal_token_id,
		dao_bulla,
		proposal_blind,
	);
```

We create a proposal which will send tokens to the dest provided.
This will soon be changed to be generic. Proposals will commit to
calling params or code instead.

## `DAO::vote()`: Vote on a Proposal

Governance token holders each make an encrypted homomorphic commitment to
their vote. The homomorphism is additive so $f(u) + f(v) = f(u + v)$.
They also encrypt their vote to the DAO pubkey.

Finally once voting is completed, the holders of the DAO pubkey (which
is up to DAO policy) can decrypt the votes $f(v₁), …, f(vₙ)$, sum the values
$v₁ + ⋯ + vₙ$ and so have the value which can be used in ZK proofs alongside
the publicly available commitment $f(v₁ + ⋯ + vₙ) = f(v₁) + ⋯ + f(vₙ)$.

## `DAO::exec()`: Execute Passed Proposal

This is the key part. We produce a tx which has two contract calls:
`[money::transfer(), DAO::exec()]`. The coins spent in `money::transfer()`
belong to the DAO and have the condition that they can only be spent
when combined with `DAO::exec()`. Here is what coins in `money::transfer()`
look like:

```
	C = poseidon_hash(
		pub_x,
		pub_y,
		value,
		token,
		serial,
		spend_hook,
		user_data,
	);
```

When we send coins to the DAO treasury, we set `spend_hook` to the DAO contract,
and `user_data` to the DAO bulla.

When spending the coins, they reveal the `spend_hook` publicly and `user_data`
(encrypted). `money::transfer()` enforces that the next contract call must be
the same as the `spend_hook`.

The contract invoked by `spend_hook` can then use the `user_data`. We use this
to store the DAO bulla. `DAO::exec()` will then use this as our DAO, and check
the proposal we are executing belongs to this DAO through the reference to
the DAO bulla in the proposal params.

`DAO::exec()` then encodes the rules that specify there has to be a valid
proposal where voting passed the threshold and so on.

Assuming both contracts validate successfully, the funds are transferred out
of the DAO treasury.

# Formalism

Let the $ℂ$ be the category for all sets of coins $C$ with one-way arrows
$C → C'$ such that $C ⊆ C'$ and an initial object $C₀ = ∅ $.
We require that arrows with the same source and target commute.
$$ \begin{CD}
   C @>c_b>> C_b \\
@VcₐVV @Vc_a'VV \\
   Cₐ @>c_b'>> C_{ab}
\end{CD} $$

We define the nullifier functor $N : ℂ^{\t{op}} → ℕ$ which is an isomorphism
of $ℂ$ that reverses arrows.

$$ \begin{CD}
   C @>>> NC \\
@VcVV @AANcA \\
   C' @>>> NC'
\end{CD} $$
We can see the action of adding $c$ to $C$ (expressed as the left downwards
arrow) gets lifted to the arrow going backwards in the nullifier category.
The collection of arrows in $ℂ$ and $ℕ$ then describes the coins and nullifier
sets which are represented in merkle trees.

From the diagram we see that $C → C' → NC' → NC → C$ so that $Nc$ cancels $c$.
Pasting diagrams together, we get

$$ \begin{CD}
   C₀ @>>> NC₀ \\
@Vc₁VV @AANc₁A \\
   C₁ @>>> NC₁ \\
@Vc₂VV @AANc₂A \\
   C₂ @>>> NC₂ \\
\end{CD} $$
where all squares commute. Since all paths in $ℂ$ are one way, proving a
coin $cₖ : Cₖ₋₁ → Cₖ$ exists is equivalent to being at any state $Cₖ, Cₖ₊₁, Cₖ₊₂, …$.

**Lemma:** If our state is $Cₖ$ then our set must contain the coins
represented as arrows $c₁, …, cₖ$.

# Anon Voting Mechanics

When making a proposal, we need to prove ownership of a threshold of coins.
Likewise for voting. Essentially they are similar problems of proving ownership
of a coin $c$ that is still valid. As showed above this reduces to the following
statements:

* Is $c$ in the set of all coins $C$?
* If yes, then is $n(c)$ *not* in the set of nullifiers $N$?

Normally this logic is handled by transfers, but we need to additionally
check it without leaking info about $c$. Since $n(c)$ is derived
deterministically, leaking $n(c)$ also leaks info on $c$.

Nullifiers must be checked otherwise expired coins can be used.

## Forking the Global State

The first method involves copying the coins state $C$. Every proof makes use
of $C$ while revealing $n(c)$ which is checked against the current nullifier
state. To avoid anonymity leaks from revealing $n(c)$, we additionally move the coin
using a `Money::transfer()` call.

The downside is that wallets need to:

* Keep track of the coins tree $C$. This will involve logic to periodically
  checkpoint the incremental tree in a deterministic way.
* When doing any action, the wallet must move coins simultaneously.
  Wallets must also keep track of the unspent coin since for example it might
  be used in another vote (or the wallet makes a proposal and wants to vote
  with the same coin).

Additionally you cannot obtain a coin then vote. You must own the coin before
the vote is proposed.

## Forking the Global State (with SMT)

Instead of revealing the nullifier, we instead snapshot the the nullifier
tree alongside $C$.

The downsides are:

* More expensive for voters since SMT is expensive in ZK.
* We're taking an older snapshot of the coins state. Spent coins spent after
  the vote are proposed will still be able to vote.

## Tracking Coins with Local State

Each coin's `user_data` contains an SMT of all proposals they voted in.
When transferring a coin, you must preserve this `user_data`.
The `spend_hook` only allows modifying it with a parent call that adds
proposals when voting to the SMT field in the coin.

The downside for wallets is that:

* The SMT committed to is large and needs to be transferred to receivers
  when sending the coin.
    * Alternatively coins could contain a special key (also in the `user_data`
      field), which when voting you must make a verifiable encryption.
      That way wallets can later scan all proposals for a DAO to find where
      their particular governance token voted.
* It's very complex. For example, can DAOs own governance tokens? So far DAO
  tokens must have the `spend_hook` set, but with this, we now require another
  `spend_hook` which preserves the SMT when transferring coins. The mechanics
  for two parents of a call aren't specified, so we'd maybe have to add some
  concept of symlinks.

However while complex, it is the most accurate of all 3 methods reflecting
the current state.

# Tree States On Disk

This section is not specific to DAO or Money, but describes a generic set
abstraction which you can add or remove items from.

Requirements:

* Add and remove items, which is represented by two G-sets denoted coins $Cₖ$
  and nullifiers $Nₖ$. Let $Rₖ$ and $Sₖ$ be commitments to them both
  respectively.
* Given any snapshot $(Rₖ, Sₖ)$, it's possible to definitively say whether the
  set it represents contains an item $x$ or not.
    * Be able to do this fully ZK.
* Wallets can easily take any snapshot, and using delta manipulation of the
  state, be able to make inclusion and exclusion proofs easily.
* Verify in WASM that $Rₖ$ and $Sₖ$ correspond to the same $k$.

The proposal is as follows and involves a merkle tree $𝐂$, and a SMT $𝐍$.

For the sake of clarity, we will not detail the storing of the trees themselves.
For $𝐂$, the tree is stored in `db_info`, while $𝐍$ has a full on-disk
representation. Instead the info in this section concerns the auxilliary data
required for using the trees with snapshotted states.

## DB Merkle Roots

This is used to quickly lookup a state commitment for $𝐂$ and figure out when it
occurred.

| Key or Value | Field Name   | Size | Desc                       |
|--------------|--------------|------|----------------------------|
| k            | Root         | 32   | The current root hash $Rₖ$ |
| v            | Block height | 3    | Current block height       |
| v            | Tx index     | 2    | Tx index                   |
| v            | Call index   | 2    | Index of contract call     |

Note: 3 bytes for blockheight can store 50 years worth of blocks.

We use the `(block_height, tx_index)` tuple to figure out all info about
this state change (such as when it occurred).
We can even get the tx itself if desired.

## DB SMT Roots

Just like for the merkle case, we want to quickly see whether $Rₖ$ and
$Sₖ$ correspond to each other. We then use the table to lookup
`(block_height, tx_index, call_index)` and check they match.
If so, then they both exist in the same `update()` call.

| Key or Value | Field Name   | Size | Desc                       |
|--------------|--------------|------|----------------------------|
| k            | Root         | 32   | The current root hash $Sₖ$ |
| v            | Block height | 3    | Current block height       |
| v            | Tx index     | 2    | Tx index                   |
| v            | Call index   | 2    | Index of contract call     |

## DB Coins (Wallets)

This DB is maintained by the user wallet, and periodic garbage collection will
remove values older than a cutoff.

Keeps track of values added to $𝐂$ or $𝐍$.

For $𝐂$ given an earlier tree checkpoint state, we can rewind, then fast forward
to have a valid merkle tree for the given snapshot.
Wallets should additionally periodically copy the merkle tree $𝐂$.

In the case of $𝐍$, we construct an overlay for SMT, which allows rewinding the
tree so exclusion proofs can be constructed.

| Key or Value | Field Name   | Size | Desc                                  |
|--------------|--------------|------|---------------------------------------|
| k            | Block height | 3    | Block height for coin                 |
| k            | Tx index     | 2    | Index in block for tx containing coin |
| k            | Call index   | 2    | Index of contract call                |
| k            | Val index    | 2    | Index of this coin or nullifier       |
| v            | Value        | 32   | Coin or nullifier                     |
| v            | Type         | 1    | Single byte indicating the type       |

This structure for the keys in an ordered B-Tree, means it can be iterated
from any point. We can start from any location from our last stored merkle
tree checkpoint, and iterate forwards adding coins until we reach our
desired snapshot $(Rₖ, Sₖ)$. We then have a valid merkle tree and SMT
reconstructed and can create the desired inclusion or exclusion proofs.

Q: should this be 2 databases or one? If we use 2 then we can remove the type
byte. Maybe more conceptually clearer?

