# Scheme

<!-- toc -->

Let $\t{PoseidonHash}$ be defined as in the section [PoseidonHash Function](../../crypto-schemes.md#poseidonhash-function).

Let $𝔽ₚ, ℙₚ, \t{DerivePubKey}, \t{Lift}_q, G_N, \mathcal{X}, \mathcal{Y}$ be defined as in the section [Pallas and Vesta](../../crypto-schemes.md#pallas-and-vesta).

Let $\t{PedersenCommit}$ be defined as in the section [Homomorphic Pedersen Commitments](../../crypto-schemes.md#homomorphic-pedersen-commitments).

Let $\t{MerklePos}, \t{MerklePath}, \t{MerkleRoot}$ be defined as in the section [Incremental Merkle Tree](../../crypto-schemes.md#incremental-merkle-tree).

Let $\t{Params}_\t{DAO}, \t{Bulla}_\t{DAO}, \t{Params}_\t{Proposal}, \t{Bulla}_\t{Proposal}$ be defined as in [DAO Model](model.md).

Let $\t{AeadEncNote}$ be defined as in [In-band Secret Distribution](../../crypto-schemes.md#in-band-secret-distribution).

Let $\t{ElGamalEncNote}ₖ, \t{ElGamal}.\t{Encrypt}$ be defined as in the section [Verifiable In-Band Secret Distribution](../../crypto-schemes.md#verifiable-in-band-secret-distribution).

## Mint

This function creates a DAO bulla $𝒟$. It's comparatively simple- we commit to
the DAO params and then add the bulla to the set.

* Wallet builder: `src/contract/dao/src/client/mint.rs`
* WASM VM code: `src/contract/dao/src/entrypoint/mint.rs`
* ZK proof: `src/contract/dao/proof/mint.zk`

### Function Params

Define the DAO mint function params
$$ \begin{aligned}
  𝒟 &∈ \t{im}(\t{Bulla}_\t{DAO}) \\
  \t{PK} &∈ ℙₚ
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-mint-params}}
```

### Contract Statement

**DAO bulla uniqueness** &emsp; whether $𝒟$ already exists. If yes then fail.

Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  L &∈ ℕ₆₄ \\
  Q &∈ ℕ₆₄ \\
  EEQ &∈ ℕ₆₄ \\
  A_q &∈ ℕ₆₄ \\
  A_b &∈ ℕ₆₄ \\
  τ &∈ 𝔽ₚ \\
  Nx &∈ 𝔽ₚ \\
  px &∈ 𝔽ₚ \\
  Px &∈ 𝔽ₚ \\
  Vx &∈ 𝔽ₚ \\
  Ex &∈ 𝔽ₚ \\
  EEx &∈ 𝔽ₚ \\
  b_\t{DAO} &∈ 𝔽ₚ
\end{aligned} $$
Attach a proof $π$ such that the following relations hold:

**Proof of notes public key ownership** &emsp; $\t{NPK} = \t{DerivePubKey}(Nx)$.

**Proof of proposer public key ownership** &emsp; $\t{pPK} = \t{DerivePubKey}(px)$.

**Proof of proposals public key ownership** &emsp; $\t{PPK} = \t{DerivePubKey}(Px)$.

**Proof of votes public key ownership** &emsp; $\t{VPK} = \t{DerivePubKey}(Vx)$.

**Proof of executor public key ownership** &emsp; $\t{EPK} = \t{DerivePubKey}(Ex)$.

**Proof of early executor public key ownership** &emsp; $\t{EEPK} = \t{DerivePubKey}(EEx)$.

**Early execution quorum is not less than quorum** &emsp; $Q ≤ EEQ$.

**DAO bulla integrity** &emsp; $𝒟 = \t{Bulla}_\t{DAO}((L, Q, EEQ, A_q, A_b, τ,
\t{NPK}, \t{pPK}, \t{PPK}, \t{VPK}, \t{EPK}, \t{EEPK}), b_\t{DAO})$

The state update additionally appends $𝒟$ to the DAO bulla Merkle tree,
which is used later for existence proofs in the Propose phase.

### Signatures

There should be a single signature attached, which uses
$\t{NPK}$ as the signature public key.

## Propose

This contract function creates a DAO proposal. It takes a merkle root
$R_\t{DAO}$ which contains the DAO bulla created in the Mint phase.

Several inputs are attached containing proof of ownership for the governance
token. This is to satisfy the proposer limit value set in the DAO.
We construct the proposal-specific nullifier $\cN$ which can leak anonymity
when those same coins are spent. To workaround this, wallet implementers
can attach an additional `Money::transfer()` call to the transaction.

For every input, an SMT non-membership proof shows the corresponding coin
was unspent in the Money state snapshot determined by the coin merkle root
$R_\t{coin}$ and the nullifier set SMT root $R_\t{null}$. Each value commit
$V$ exported by the inputs is summed and used in the main proof to determine
that the total value attached in the inputs crosses the proposer limit
threshold.

This is merely a proof of ownership of holding a certain amount of value.
Coins are not locked and continue to be spendable.

Additionally the encrypted note $\t{note}$ is used to send the proposal
values to the DAO members using the public key set inside the DAO.

A proposal contains a list of auth calls as specified in [Auth Calls](model.md#auth-calls). This specifies the contract call executed by the DAO on passing.

* Wallet builder: `src/contract/dao/src/client/propose.rs`
* WASM VM code: `src/contract/dao/src/entrypoint/propose.rs`
* ZK proofs:
  * `src/contract/dao/proof/propose-main.zk`
  * `src/contract/dao/proof/propose-input.zk`

### Function Params

Define the DAO propose function params
$$ \begin{aligned}
  R_\t{DAO} &∈ 𝔽ₚ \\
  R_\t{coin} &∈ 𝔽ₚ \\
  R_\t{null} &∈ 𝔽ₚ \\
  T &∈ 𝔽ₚ \\
  𝒫 &∈ \t{im}(\t{Bulla}_\t{Proposal}) \\
  \t{note} &∈ \t{AeadEncNote} \\
  𝐢 &∈ \t{ProposeInput}^*
\end{aligned} $$

Define the DAO propose-input function params
$$ \begin{aligned}
  \t{ProposeInput}.\cN &∈ 𝔽ₚ \\
  \t{ProposeInput}.V &∈ ℙₚ \\
  \t{ProposeInput}.\t{PK}_σ &∈ ℙₚ
\end{aligned} $$

The coin merkle root $R_\t{coin}$ and nullifier SMT root $R_\t{null}$ are
attached once per call and bound to every input proof.

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-propose-params}}
```

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-propose-params-input}}
```

### Contract Statement

Let $t₀ = \t{BlockWindow} ∈ 𝔽ₚ$ be the current blockwindow as defined in [Blockwindow](model.md#blockwindow).

Let $\t{Attrs}_\t{Coin}$ be defined as in [Coin](../money/model.md#coin).

**Valid DAO bulla merkle root** &emsp; check that $R_\t{DAO}$ is a previously
seen merkle root in the DAO contract merkle roots DB.

**Valid snapshot** &emsp; check that $R_\t{coin}$ is a previously seen
merkle root in the money contract merkle roots DB, that $R_\t{null}$ is a
previously seen SMT root in the money contract nullifier roots DB, and
that the two snapshots correspond to the same state (the nullifier SMT
root snapshot must contain the coin tree root snapshot). The snapshot
must be recent enough: not older than `PROPOSAL_SNAPSHOT_CUTOFF_LIMIT`
blocks (100).

**Proposal bulla uniqueness** &emsp; whether $𝒫$ already exists. If yes then fail.

Let there be prover auxiliary witness inputs:
$$ \begin{aligned}
  v &∈ 𝔽ₚ \\
  bᵥ &∈ 𝔽ᵥ \\
  b_τ &∈ 𝔽ₚ \\
  p &∈ \t{Params}_\t{Proposal} \\
  b_p &∈ 𝔽ₚ \\
  d &∈ \t{Params}_\t{DAO} \\
  b_d &∈ 𝔽ₚ \\
  px &∈ 𝔽ₚ \\
  (ψ, Π) &∈ \t{MerklePos} × \t{MerklePath} \\
\end{aligned} $$
Attach a proof $π_𝒫$ such that the following relations hold:

**Governance token commit** &emsp; export the DAO token ID as an encrypted
commit $T = \t{PoseidonHash}(d.τ, b_τ)$, matching the token commit of
every input.

**Proof of proposer public key ownership** &emsp; $\t{pPK} = \t{DerivePubKey}(px)$.

**DAO bulla integrity** &emsp; $𝒟 = \t{Bulla}_\t{DAO}(d, b_d)$

**DAO existence** &emsp; $R_\t{DAO} = \t{MerkleRoot}(ψ, Π, 𝒟)$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, b_p)$
where $p.t₀ = t₀$ is enforced as a public input.

**Proposer limit threshold met** &emsp; check the proposer has supplied enough
inputs that the required funds for the proposer limit set in the DAO is met.
Let the total funds $v = ∑_{i ∈ 𝐢} i.v$, then check $d.L ≤ v$.

**Total funds value commit** &emsp; $V = \t{PedersenCommit}(v, bᵥ)$ where
$V = ∑_{i ∈ 𝐢} i.V$. We use this to check that $v = ∑_{i ∈ 𝐢} i.v$ as
claimed in the *proposer limit threshold met* check.

**Input uniqueness** &emsp; the nullifiers $\cN$ must be unique within the call.

For each input $i ∈ 𝐢$, perform the following checks:

&emsp; Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  x_c &∈ 𝔽ₚ \\
  c &∈ \t{Attrs}_\t{Coin} \\
  bᵥ &∈ 𝔽ᵥ \\
  b_τ &∈ 𝔽ₚ \\
  (ψᵢ, Πᵢ) &∈ \t{MerklePos} × \t{MerklePath} \\
  (ψ^N, Π^N) &∈ \t{MerklePos} × \t{MerklePath} \\
  x_σ &∈ 𝔽ₚ \\
\end{aligned} $$
&emsp; Attach a proof $π_i$ such that the following relations hold:

&emsp; **Nullifier integrity** &emsp; let $C = \t{Coin}(c)$ and
$N = \t{PoseidonHash}(x_c, C)$, then $\cN = \t{PoseidonHash}(N, 𝒫)$

&emsp; **Unspent at snapshot** &emsp;
$R_\t{null} = \t{MerkleRoot}(ψ^N, Π^N, 0)$, i.e. an SMT non-membership
proof of $N$ in the nullifier set snapshot.

&emsp; **Coin value commit** &emsp; $i.V = \t{PedersenCommit}(c.v, bᵥ)$.

&emsp; **Token commit** &emsp; $T = \t{PoseidonHash}(c.τ, b_τ)$.

&emsp; **Valid coin** &emsp; Check $c.P = \t{DerivePubKey}(x_c)$. Check $R_\t{coin} = \t{MerkleRoot}(ψᵢ, Πᵢ, C)$.

&emsp; **Proof of signature public key ownership** &emsp; $i.\t{PK}_σ = \t{DerivePubKey}(x_σ)$.

**Snapshot creation** &emsp; once the proposal is accepted, the latest
Money coin merkle root and nullifier SMT root are snapshotted alongside
the proposal. Only coins in this snapshot are votable with (see [Vote](#vote)).

### Signatures

For each $i ∈ 𝐢$, attach a signature corresponding to the
public key $i.\t{PK}_σ$.

## Vote

After `DAO::propose()` is called, DAO members can then call this contract
function. Using a similar method as before, they attach inputs proving ownership
of a certain value of governance tokens. This is how we achieve token weighted
voting. The result of the vote is communicated to DAO members that can view votes
through the encrypted note $\t{note}$.

Each nullifier $𝒩$ is stored uniquely per proposal. Additionally as before,
there is a leakage here connecting the coins when spent. However prodigious
usage of `Money::transfer()` to wash the coins after calling `DAO::vote()`
should mitigate against this attack. In the future this can be fixed using
set non-membership primitives.

Another leakage is that the proposal bulla $𝒫$ is public. To ensure every vote
is discoverable by verifiers (who cannot decrypt values) and protect against
'nothing up my sleeve', we link them all together. This is so the final tally
used for executing proposals is accurate.

The total sum of votes is represented by the commit $V_\t{all} = ∑_{i ∈ 𝐢} i.V$
and the yes votes by $V_\t{yes}$.

* Wallet builder: `src/contract/dao/src/client/vote.rs`
* WASM VM code: `src/contract/dao/src/entrypoint/vote.rs`
* ZK proofs:
  * `src/contract/dao/proof/vote-main.zk`
  * `src/contract/dao/proof/vote-input.zk`

### Function Params

Define the DAO vote function params
$$ \begin{aligned}
  T &∈ 𝔽ₚ \\
  𝒫 &∈ \t{im}(\t{Bulla}_\t{Proposal}) \\
  V_\t{yes} &∈ ℙₚ \\
  \t{note} &∈ \t{ElGamalEncNote}₄ \\
  𝐢 &∈ \t{VoteInput}^*
\end{aligned} $$

Define the DAO vote-input function params
$$ \begin{aligned}
  \t{VoteInput}.𝒩 &∈ 𝔽ₚ \\
  \t{VoteInput}.V &∈ ℙₚ \\
  \t{VoteInput}.\t{PK}_σ &∈ ℙₚ
\end{aligned} $$

**Note**: $\t{VoteInput}.V$ is a pedersen commitment, where the blinds are
selected such that their sum is a valid field element in $𝔽ₚ$ so the blind
for $∑ V$ can be verifiably encrypted. Likewise we do the same for the blind
used to calculate $V_\t{yes}$.

This allows DAO members that hold the votes key to securely receive all secrets
for votes on a proposal. This is then used in the Exec phase when we work on the
sum of DAO votes.

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-vote-params}}
```

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-vote-params-input}}
```

### Contract Statement

Let $t₀ = \t{BlockWindow} ∈ 𝔽ₚ$ be the current blockwindow as defined in [Blockwindow](model.md#blockwindow).

Let $R_\t{coin}, R_\t{null}$ be the Money state snapshot attached to the
proposal when it was created.

**Proposal bulla exists** &emsp; check $𝒫$ exists in the DAO contract proposal
bullas DB.

Let there be prover auxiliary witness inputs:
$$ \begin{aligned}
  p &∈ \t{Params}_\t{Proposal} \\
  b_p &∈ 𝔽ₚ \\
  d &∈ \t{Params}_\t{DAO} \\
  b_d &∈ 𝔽ₚ \\
  o &∈ 𝔽ₚ \\
  b_y &∈ 𝔽ₚ \\
  v &∈ 𝔽ₚ \\
  bᵥ &∈ 𝔽ₚ \\
  b_τ &∈ 𝔽ₚ \\
  t_\t{now} &∈ 𝔽ₚ \\
  \t{esk} &∈ 𝔽ₚ \\
\end{aligned} $$
Attach a proof $π_\mathcal{V}$ such that the following relations hold:

**Governance token commit** &emsp; export the DAO token ID as an encrypted
commit $T = \t{PoseidonHash}(d.τ, b_τ)$, matching the token commit of
every input.

**DAO bulla integrity** &emsp; $𝒟 = \t{Bulla}_\t{DAO}(d, b_d)$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, b_p)$

**Yes vote commit** &emsp; $V_\t{yes} = (o \cdot v)G_V + b_y H_B$

**Total vote value commit** &emsp; $V_\t{all} = vG_V + bᵥH_B$ where
$V_\t{all} = ∑_{i ∈ 𝐢} i.V$ should also hold. Here $G_V$ is the
`VALUE_COMMIT_VALUE` generator, and the blinds $b_y, bᵥ$ are base field
elements used with the `VALUE_COMMIT_RANDOM_BASE` generator $H_B$, so
they can be verifiably encrypted.

**Vote option boolean** &emsp; enforce $o ∈ \{ 0, 1 \}$.

**Proposal not expired** &emsp; let $t_\t{end} = ℕ₆₄2𝔽ₚ(p.t₀) + ℕ₆₄2𝔽ₚ(p.D)$,
and then check $t_\t{now} < t_\t{end}$, where $t_\t{now}$ is enforced to be
the current blockwindow via a public input.

**Verifiable encryption of vote commit secrets** &emsp;
let $𝐧 = (o, b_y, v, bᵥ)$, and verify
$\t{note} = \t{ElGamal}.\t{Encrypt}(𝐧, \t{esk}, d.\t{VPK})$.

For each input $i ∈ 𝐢$, perform the following checks:

&emsp; **Unused nullifier (proposal)** &emsp; check that $𝒩$ does not exist in the
DAO contract nullifiers DB for this specific proposal (nullifiers are keyed
by $(𝒫, 𝒩)$), and is unique within the call.

&emsp; Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  x_c &∈ 𝔽ₚ \\
  c &∈ \t{Attrs}_\t{Coin} \\
  bᵥ &∈ 𝔽ᵥ \\
  b_τ &∈ 𝔽ₚ \\
  (ψᵢ, Πᵢ) &∈ \t{MerklePos} × \t{MerklePath} \\
  (ψ^N, Π^N) &∈ \t{MerklePos} × \t{MerklePath} \\
  x_σ &∈ 𝔽ₚ \\
\end{aligned} $$
Attach a proof $πᵢ$ such that the following relations hold:

&emsp; **Nullifier integrity** &emsp; let $C = \t{Coin}(c)$ and
$N = \t{PoseidonHash}(x_c, C)$, then
$𝒩 = \t{PoseidonHash}(N, x_c, 𝒫)$

&emsp; **Unspent at snapshot** &emsp;
$R_\t{null} = \t{MerkleRoot}(ψ^N, Π^N, 0)$, i.e. an SMT non-membership
proof of $N$ in the nullifier set snapshot, so only participants from
before the proposal was posted can vote.

&emsp; **Coin value commit** &emsp; $i.V = \t{PedersenCommit}(c.v, bᵥ)$.

&emsp; **Token commit** &emsp; $T = \t{PoseidonHash}(c.τ, b_τ)$.

&emsp; **Valid coin** &emsp; Check $c.P = \t{DerivePubKey}(x_c)$. Check $R_\t{coin} = \t{MerkleRoot}(ψᵢ, Πᵢ, C)$, i.e. the coin existed in the proposal's snapshot of the coin tree.

&emsp; **Proof of signature public key ownership** &emsp; $i.\t{PK}_σ = \t{DerivePubKey}(x_σ)$.

**Vote aggregation** &emsp; the state update adds $V_\t{yes}$ and
$∑ i.V$ to the proposal's aggregated vote commits
($\t{DaoBlindAggregateVote}$), and records the used vote nullifiers.

### Signatures

For each $i ∈ 𝐢$, attach a signature corresponding to the
public key $i.\t{PK}_σ$.

## Exec

Exec is the final stage after voting is [Accepted](concepts.md#proposal-states).

It checks that voting has passed, and correct conditions have been met, in accordance
with the [DAO params](model.md#dao) such as quorum and approval ratio.
$V_\t{yes}$ and $V_\t{all}$ are pedersen commits to $v_\t{yes}$ and $v_\t{all}$ respectively.

It also checks that child calls have been attached in accordance with the auth
calls set inside the proposal. One of these will usually be an auth module
function. Currently the DAO provides a single preset for executing
`Money::transfer()` calls so DAOs can manage anonymous treasuries.

The `early_exec` flag selects which proof statement is verified:
`Exec` for normal execution after expiry, or `EarlyExec` for strongly
supported proposals (see [EarlyExec](#earlyexec)).

* Wallet builder: `src/contract/dao/src/client/exec.rs`
* WASM VM code: `src/contract/dao/src/entrypoint/exec.rs`
* ZK proofs:
  * `src/contract/dao/proof/exec.zk`
  * `src/contract/dao/proof/early-exec.zk`

### Function Params

Let $\t{AuthCall}, \t{Commit}_{\t{Auth}^*}$ be defined as in the section [Auth Calls](model.md#auth-calls).

Define the DAO exec function params
$$ \begin{aligned}
  𝒫 &∈ \t{im}(\t{Bulla}_\t{Proposal}) \\
  𝒜 &∈ \t{AuthCall}^* \\
  V_\t{yes} &∈ ℙₚ \\
  V_\t{all} &∈ ℙₚ \\
  \t{early} &∈ ℤ₂ \\
  \t{PK}_σ &∈ ℙₚ
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-exec-params}}
```

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-blind-aggregate-vote}}
```

### Contract Statement

There are two phases to Exec. In the first we check the calling format of this
transaction matches what is specified in the proposal. Then in the second phase,
we verify the correct voting rules.

**Auth call spec match** &emsp; denote the child calls of Exec by $C$.
If $\#C ≠ \#𝒜$ then exit.
Otherwise, for each $c ∈ C$ and $a ∈ 𝒜$, check the contract ID and
function code of $c$ match $a$.

**Aggregate votes lookup** &emsp; using the proposal bulla, fetch the
aggregated votes from the DB and verify $V_\t{yes}$ and $V_\t{all}$ are set correctly.

Let there be prover auxiliary witness inputs:
$$ \begin{aligned}
  p &∈ \t{Params}_\t{Proposal} \\
  b_p &∈ 𝔽ₚ \\
  d &∈ \t{Params}_\t{DAO} \\
  b_d &∈ 𝔽ₚ \\
  v_y &∈ 𝔽ₚ \\
  v_a &∈ 𝔽ₚ \\
  b_y &∈ 𝔽ᵥ \\
  b_a &∈ 𝔽ᵥ \\
  t_\t{now} &∈ 𝔽ₚ \\
  x_σ &∈ 𝔽ₚ
\end{aligned} $$
Attach a proof $π$ such that the following relations hold:

**Proof of executor public key ownership** &emsp; $\t{EPK} = \t{DerivePubKey}(Ex)$.

**DAO bulla integrity** &emsp; $𝒟 = \t{Bulla}_\t{DAO}(d, b_d)$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, b_p)$
where $\t{Commit}_{\t{Auth}^*}(p.C) = \t{Commit}_{\t{Auth}^*}(𝒜)$ is
enforced as a public input.

**Proposal has expired** &emsp; let $t_\t{end} = ℕ₆₄2𝔽ₚ(p.t₀) + ℕ₆₄2𝔽ₚ(p.D)$,
and then check $t_\t{end} ≤ t_\t{now}$, where $t_\t{now}$ is enforced to
be the current blockwindow via a public input.

**Yes vote commit** &emsp; $V_\t{yes} = \t{PedersenCommit}(v_y, b_y)$

**All vote commit** &emsp; $V_\t{all} = \t{PedersenCommit}(v_a, b_a)$

**All votes pass quorum** &emsp; $Q ≤ v_a$

**Approval ratio satisfied** &emsp; we wish to check that
$\frac{A_q}{A_b} ≤ \frac{v_y}{v_a}$. Instead we perform the
equivalent check that $v_a A_q ≤ v_y A_b$.

**Proposal removal** &emsp; the state update removes the proposal from
the DB so it cannot be executed twice.

### EarlyExec

This is a special case of Exec for when we want to execute a strongly accepted proposal
before voting period has passed. A different proof statement is used in this case.

Let there be prover auxiliary witness inputs:
$$ \begin{aligned}
  p &∈ \t{Params}_\t{Proposal} \\
  b_p &∈ 𝔽ₚ \\
  d &∈ \t{Params}_\t{DAO} \\
  b_d &∈ 𝔽ₚ \\
  v_y &∈ 𝔽ₚ \\
  v_a &∈ 𝔽ₚ \\
  b_y &∈ 𝔽ᵥ \\
  b_a &∈ 𝔽ᵥ \\
  t_\t{now} &∈ 𝔽ₚ \\
  x_σ &∈ 𝔽ₚ
\end{aligned} $$
Attach a proof $π$ such that the following relations hold:

**Proof of executor public key ownership** &emsp; $\t{EPK} = \t{DerivePubKey}(Ex)$.

**Proof of early executor public key ownership** &emsp; $\t{EEPK} = \t{DerivePubKey}(EEx)$.

**DAO bulla integrity** &emsp; $𝒟 = \t{Bulla}_\t{DAO}(d, b_d)$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, b_p)$
where $\t{Commit}_{\t{Auth}^*}(p.C) = \t{Commit}_{\t{Auth}^*}(𝒜)$ is
enforced as a public input.

**Proposal has not expired** &emsp; let $t_\t{end} = ℕ₆₄2𝔽ₚ(p.t₀) + ℕ₆₄2𝔽ₚ(p.D)$,
and then check $t_\t{now} < t_\t{end}$, where $t_\t{now}$ is enforced to
be the current blockwindow via a public input.

**Yes vote commit** &emsp; $V_\t{yes} = \t{PedersenCommit}(v_y, b_y)$

**All vote commit** &emsp; $V_\t{all} = \t{PedersenCommit}(v_a, b_a)$

**All votes pass early execution quorum** &emsp; $EEQ ≤ v_a$

**Approval ratio satisfied** &emsp; we wish to check that
$\frac{A_q}{A_b} ≤ \frac{v_y}{v_a}$. Instead we perform the
equivalent check that $v_a A_q ≤ v_y A_b$.

### Signatures

A single signature is attached, using $\t{PK}_σ$ as the signature public
key. The signature binds the `DAO::exec()` call to the transaction so it
cannot be combined with other calls.

## AuthMoneyTransfer

This is a child call for Exec which can be used for DAO treasuries.
It checks the next sibling call is `Money::transfer()` and accordingly
verifies the first $n - 1$ output coins match the data set in this
call's [auth data](model.md#auth-calls).

Additionally we provide verifiably encrypted notes for the coins, to
mitigate the attack where Exec is called, but the supplied
`Money::transfer()` call contains an invalid note which cannot be
decrypted by the receiver. In this case, the money would still leave the
DAO treasury but be unspendable.

* Wallet builder: `src/contract/dao/src/client/auth_xfer.rs`
* WASM VM code: `src/contract/dao/src/entrypoint/auth_xfer.rs`
* ZK proofs:
  * `src/contract/dao/proof/auth-money-transfer.zk`
  * `src/contract/dao/proof/auth-money-transfer-enc-coin.zk`

### Function Params

Define the DAO AuthMoneyTransfer function params
$$ \begin{aligned}
  𝒞_\t{enc} &∈ \t{ElGamalEncNote}₅^* \\
  𝒟_\t{enc} &∈ \t{ElGamalEncNote}₃
\end{aligned} $$

This provides verifiable note encryption for all output coins in the sibling `Money::transfer()` call as well as the DAO change coin.

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-auth_xfer-params}}
```

### Contract Statement

Denote the DAO function ID of `Dao::Exec` by $\t{FID}_\t{Exec} ∈ 𝔽ₚ$.

**Sibling call is `Money::transfer()`** &emsp; load the immediate next
sibling call and check the contract ID and function code match
`Money::transfer()`.

**Money originates from the same DAO** &emsp; check all the input's `user_data`
for the sibling `Money::transfer()` encode the same DAO. We do this by using the
same blind for all `user_data`. Denote this value by $\t{UD}_\t{enc}$.

**Output coins match proposal** &emsp; check there are $n + 1$ output coins,
with the first $n$ coins exactly matching those set in this call's auth
data in the parent `DAO::exec()` call. The auth data is decoded as a
list of $n$ coins.

Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  p &∈ \t{Params}_\t{Proposal} \\
  b_p &∈ 𝔽ₚ \\
  d &∈ \t{Params}_\t{DAO} \\
  b_d &∈ 𝔽ₚ \\
  b_\t{UD} &∈ 𝔽ₚ \\
  v_\t{DAO} &∈ 𝔽ₚ \\
  τ_\t{DAO} &∈ 𝔽ₚ \\
  b_\t{DAO} &∈ 𝔽ₚ \\
  \t{esk} &∈ 𝔽ₚ \\
\end{aligned} $$

Attach a proof $π_\t{auth}$ such that the
following relations hold:

**DAO bulla integrity** &emsp; $𝒟 = \t{Bulla}_\t{DAO}(d, b_d)$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, b_p)$
where $𝒫$ matches the value in `DAO::exec()`, and
$\t{Commit}_{\t{Auth}^*}(p.C)$ is enforced as a public input.

**Input user data commits to DAO bulla** &emsp; $\t{UD}_\t{enc} =
\t{PoseidonHash}(𝒟, b_\t{UD})$

**DAO change coin integrity** &emsp; denote the last coin in the
`Money::transfer()` outputs by $C_\t{DAO}$. Then check
$$ C_\t{DAO} = \t{Coin}(d.\t{NPK}, v_\t{DAO}, τ_\t{DAO},
                        \t{FID}_\t{Exec}, 𝒟, b_\t{DAO}) $$
i.e. the change is sent back to the DAO's notes public key, locked to
`Dao::Exec` via the spend hook, with the DAO bulla as user data. The
spend hook $\t{FID}_\t{Exec}$ is additionally enforced as a public input.

**Verifiable DAO change coin note encryption** &emsp;
let $𝐧 = (v_\t{DAO}, τ_\t{DAO}, b_\t{DAO})$, and verify
$𝒟_\t{enc} = \t{ElGamal}.\t{Encrypt}(𝐧, \t{esk}, d.\t{NPK})$.

Then we do the same for each output coin of `Money::transfer()`.
For $k ∈ [n+1]$, let $a = (𝒞_\t{enc})ₖ$ and $C$ be the $k$th output coin from
`Money::transfer()`.
Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  c &∈ \t{Attrs}_\t{Coin} \\
\end{aligned} $$
Attach a proof $πₖ$ (reusing the same $\t{esk}$) such that the following
relations hold:

&emsp; **Coin integrity** &emsp; $C = \t{Coin}(c)$

&emsp; **Verifiable output coin note encryption** &emsp;
let $𝐧 = (c.v, c.τ, c.\t{SH}, c.\t{UD}, c.b)$, and verify
$a = \t{ElGamal}.\t{Encrypt}(𝐧, \t{esk}, c.\t{PK})$, i.e. the coin
secrets are encrypted to the coin's own recipient public key.

### Signatures

No signatures are attached.
