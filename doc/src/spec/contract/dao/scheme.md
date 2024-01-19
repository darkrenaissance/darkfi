# Scheme

<!-- toc -->

Let $\t{PoseidonHash}$ be defined as in the section [PoseidonHash Function](../../crypto-schemes.md#poseidonhash-function).

Let $𝔽ₚ, ℙₚ, \t{DerivePubKey}$ be defined as in the section [Pallas and Vesta](../../crypto-schemes.md#pallas-and-vesta).

Let $\t{PedersenCommit}$ be defined as in the section [Homomorphic Pedersen Commitments](../../crypto-schemes.md#homomorphic-pedersen-commitments).

Let $\t{MerklePos}, \t{MerklePath}, \t{MerkleRoot}$ be defined as in the section [Incremental Merkle Tree](../../crypto-schemes.md#incremental-merkle-tree).

Let $\t{Params}_\t{DAO}, \t{Bulla}_\t{DAO}, \t{Params}_\t{Proposal}, \t{Bulla}_\t{Proposal}$ be defined as in [DAO Model](model.md).

## Mint

### Function Params

Define the DAO mint function params
$$ \begin{aligned}
  ℬ  &∈ \t{im}(\t{Bulla}_\t{DAO}) \\
  \t{PK} &∈ ℙₚ
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-mint-params}}
```

### Contract Statement

**DAO bulla uniqueness** &emsp; whether $ℬ $ already exists. If yes then fail.

Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  \t{Params}_\t{DAO}.L &∈ ℕ₆₄ \\
  \t{Params}_\t{DAO}.Q &∈ ℕ₆₄ \\
  \t{Params}_\t{DAO}.A^\% &∈ ℕ₆₄ × ℕ₆₄ \\
  \t{Params}_\t{DAO}.T &∈ 𝔽ₚ \\
  x &∈ 𝔽ₚ \\
  b_\t{DAO} &∈ 𝔽ₚ
\end{aligned} $$

Attach a proof $π = \{ 𝐯, 𝐱 : R(𝐯, 𝐱) = 1 \}$ such that the
following relations hold:

**Proof of public key ownership** &emsp; $\t{PK} = \t{DerivePubKey}(x)$.

**DAO bulla integrity** &emsp; $ℬ  = \t{Bulla}_\t{DAO}(\t{Params}_\t{DAO}, b_\t{DAO})$

### Signatures

There should be a single signature attached, which uses
$\t{PK}$ as the signature public key.

## Propose

### Function Params

Define the DAO propose function params
$$ \begin{aligned}
  R_\t{DAO} &∈ 𝔽ₚ \\
  T &∈ 𝔽ₚ \\
  𝒫 &∈ \t{im}(\t{Bulla}_\t{Proposal}) \\
  \t{EncNote} &∈ ⟂ \\
  𝐢 &∈ \t{ProposeInput}^*
\end{aligned} $$

Define the DAO propose-input function params
$$ \begin{aligned}
  \t{ProposeInput}.\cN &∈ 𝔽ₚ \\
  \t{ProposeInput}.V &∈ ℙₚ \\
  \t{ProposeInput}.R_\t{coin} &∈ 𝔽ₚ \\
  \t{ProposeInput}.\t{PK}_σ &∈ ℙₚ
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-propose-params}}
```

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-propose-params-input}}
```

### Contract Statement

Let $t₀ = \t{CurrentDay} ∈ 𝔽ₚ$ be the current day as defined in [Current Day](model.md#current-day).

Let $\t{Attrs}_\t{Coin}$ be defined as in [Coin](../money/model.md#coin).

**Valid DAO bulla merkle root** &emsp; check that $R_\t{DAO}$ is a previously
seen merkle root in the DAO contract merkle roots DB.

**Proposal bulla uniqueness** &emsp; whether $𝒫 $ already exists. If yes then fail.

Let there be prover auxiliary witness inputs:
$$ \begin{aligned}
  v &∈ 𝔽ₚ \\
  bᵥ &∈ 𝔽ᵥ \\
  b_τ &∈ 𝔽ₚ \\
  p &∈ \t{Params}_\t{Proposal} \\
  b_p &∈ 𝔽ₚ \\
  d &∈ \t{Params}_\t{DAO} \\
  b_d &∈ 𝔽ₚ \\
  (ψ, Π) &∈ \t{MerklePos} × \t{MerklePath} \\
\end{aligned} $$
Attach a proof $π_𝒫 $ such that the following relations hold:

**Governance token commit** &emsp; export the DAO token ID as an encrypted pedersen
commit $T = \t{PedersenCommit}(d.τ, b_τ)$ where $T = ∑_{i ∈ 𝐢} Tᵢ$.

**DAO bulla integrity** &emsp; $𝒟  = \t{Bulla}_\t{DAO}(d, b_d)$

**DAO existence** &emsp; $R_\t{DAO} = \t{MerkleRoot}(ψ, Π, 𝒟 )$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, b_p)$
where $p.t₀ = t₀$.

**Proposer limit threshold met** &emsp; check the proposer has supplied enough
inputs that the required funds for the proposer limit set in the DAO is met.
Let the total funds $v = ∑_{i ∈ 𝐢} i.v$, then check $d.L ≤ v$.

**Total funds value commit** &emsp; $V = \t{PedersenCommit}(v, bᵥ)$ where
$V = ∑_{i ∈ 𝐢} i.V$. We use this to check that $v = ∑_{i ∈ 𝐢} i.v$ as
claimed in the *proposer limit threshold met* check.

For each input $i ∈ 𝐢$, perform the following checks:

&emsp; **Unused nullifier** &emsp; check that $\cN$ does not exist in the
money contract nullifiers DB.

&emsp; **Valid input coins merkle root** &emsp; check that $i.R_\t{coin}$ is a
previously seen merkle root in the money contract merkle roots DB.

&emsp; Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  x_c &∈ 𝔽ₚ \\
  c &∈ \t{Attrs}_\t{Coin} \\
  bᵥ &∈ 𝔽ᵥ \\
  b_τ &∈ 𝔽ₚ \\
  (ψᵢ, Πᵢ) &∈ \t{MerklePos} × \t{MerklePath} \\
  x_σ &∈ 𝔽ₚ \\
\end{aligned} $$
&emsp; Attach a proof $π_i$ such that the following relations hold:

&emsp; **Nullifier integrity** &emsp; $\cN = \t{PoseidonHash}(x_c, C)$

&emsp; **Coin value commit** &emsp; $i.V = \t{PedersenCommit}(c.v, bᵥ)$.

&emsp; **Token commit** &emsp; $T = \t{PoseidonHash}(c.τ, b_τ)$.

&emsp; **Valid coin** &emsp; Check $c.P = \t{DerivePubKey}(x_c)$. Let $C = \t{Coin}(c)$. Check $i.R_\t{coin} = \t{MerkleRoot}(ψᵢ, Πᵢ, C)$.

&emsp; **Proof of signature public key ownership** &emsp; $i.\t{PK}_σ = \t{DerivePubKey}(x_σ)$.

## Vote

### Function Params

Define the DAO vote function params
$$ \begin{aligned}
  τ &∈ 𝔽ₚ \\
  𝒫 &∈ \t{im}(\t{Bulla}_\t{Proposal}) \\
  V_\t{yes} &∈ ℙₚ \\
  \t{EncNote} &∈ ⟂ \\
  𝐢 &∈ \t{VoteInput}^*
\end{aligned} $$

Define the DAO vote-input function params
$$ \begin{aligned}
  \t{VoteInput}.𝒩 &∈ 𝔽ₚ \\
  \t{VoteInput}.V &∈ ℙₚ \\
  \t{VoteInput}.R_\t{coin} &∈ 𝔽ₚ \\
  \t{VoteInput}.\t{PK}_σ &∈ ℙₚ
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-vote-params}}
```

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-vote-params-input}}
```

### Contract Statement

**Proposal bulla exists** &emsp; check $𝒫 $ exists in the DAO contract proposal
bullas DB.

Let there be prover auxiliary witness inputs:
$$ \begin{aligned}
  p &∈ \t{Params}_\t{Proposal} \\
  b_p &∈ 𝔽ₚ \\
  d &∈ \t{Params}_\t{DAO} \\
  b_d &∈ 𝔽ₚ \\
  o &∈ 𝔽ₚ \\
  b_y &∈ 𝔽ᵥ \\
  v &∈ 𝔽ₚ \\
  bᵥ &∈ 𝔽ᵥ \\
  b_τ &∈ 𝔽ₚ \\
  t_\t{now} &∈ 𝔽ₚ
\end{aligned} $$
Attach a proof $π_\mathcal{V}$ such that the following relations hold:

**Governance token commit** &emsp; export the DAO token ID as an encrypted pedersen
commit $T = \t{PedersenCommit}(d.τ, b_τ)$ where $T = ∑_{i ∈ 𝐢} Tᵢ$.

**DAO bulla integrity** &emsp; $𝒟 = \t{Bulla}_\t{DAO}(d, b_d)$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, b_p)$

**Yes vote commit** &emsp; $V_\t{yes} = \t{PedersenCommit}(ov, b_y)$

**Total vote value commit** &emsp; $V = \t{PedersenCommit}(v, bᵥ)$ where
$V = ∑_{i ∈ 𝐢} i.V$ should also hold.

**Vote option boolean** &emsp; enforce $o ∈ \{ 0, 1 \}$.

**Proposal not expired** &emsp; let $t_\t{end} = ℕ₆₄2𝔽ₚ(p.t₀) + ℕ₆₄2𝔽ₚ(p.D)$,
and then check $t_\t{now} < t_\t{end}$.

For each input $i ∈ 𝐢$, perform the following checks:

&emsp; **Valid input merkle root** &emsp; check that $i.R_\t{coin}$ is the
previously seen merkle root in the proposal snapshot merkle root.

&emsp; **Unused nullifier (money)** &emsp; check that $\cN$ does not exist in the
money contract nullifiers DB.

&emsp; **Unused nullifier (proposal)** &emsp; check that $\cN$ does not exist in the
DAO contract nullifiers DB for this specific proposal.

Let there be prover auxiliary witness inputs:
$$ \begin{aligned}
  x_c &∈ 𝔽ₚ \\
  c &∈ \t{Attrs}_\t{Coin} \\
  bᵥ &∈ 𝔽ᵥ \\
  b_τ &∈ 𝔽ₚ \\
  (ψᵢ, Πᵢ) &∈ \t{MerklePos} × \t{MerklePath} \\
  x_σ &∈ 𝔽ₚ \\
\end{aligned} $$
Attach a proof $πᵢ$ such that the following relations hold:

&emsp; **Nullifier integrity** &emsp; $\cN = \t{PoseidonHash}(x_c, C)$

&emsp; **Coin value commit** &emsp; $i.V = \t{PedersenCommit}(c.v, bᵥ)$.

&emsp; **Token commit** &emsp; $T = \t{PoseidonHash}(c.τ, b_τ)$.

&emsp; **Valid coin** &emsp; Check $c.P = \t{DerivePubKey}(x_c)$. Let $C = \t{Coin}(c)$. Check $i.R_\t{coin} = \t{MerkleRoot}(ψᵢ, Πᵢ, C)$.

&emsp; **Proof of signature public key ownership** &emsp; $i.\t{PK}_σ = \t{DerivePubKey}(x_σ)$.

## Exec

### Function Params

Let $\t{AuthCall}, \t{Commit}_{\t{Auth}^*}$ be defined as in the section [Auth Calls](model.md#auth-calls).

Define the DAO exec function params
$$ \begin{aligned}
  𝒫 &∈ \t{im}(\t{Bulla}_\t{Proposal}) \\
  𝒜  &∈ \t{AuthCall}^* \\
  V_\t{yes} &∈ ℙₚ \\
  V_\t{all} &∈ ℙₚ \\
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
If $\#C ≠ \#𝒜 $ then exit.
Otherwise, for each $c ∈ C$ and $a ∈ 𝒜 $, check the function ID of $c$ is $a$.

**Aggregate votes lookup** &emsp; using the proposal bulla, fetch the
aggregated votes from the DB and verify $V_y$ and $V_a$ are set correctly.

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
\end{aligned} $$
Attach a proof $π$ such that the following relations hold:

**DAO bulla integrity** &emsp; $𝒟 = \t{Bulla}_\t{DAO}(d, b_d)$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, b_p)$
where $p.𝒜  = 𝒜 $.

**Yes vote commit** &emsp; $V_\t{yes} = \t{PedersenCommit}(v_y, b_y)$

**All vote commit** &emsp; $V_\t{yes} = \t{PedersenCommit}(v_a, b_a)$

**All votes pass quorum** &emsp; $Q ≤ v_a$

**Approval ratio satisfied** &emsp; we wish to check that
$\frac{A^\%_q}{A^\%_b} ≤ \frac{v_y}{v_a}$. Instead we perform the
equivalent check that $v_a A^\%_q ≤ v_y A^\%_b$.

## AuthMoneyTransfer

This is a child call for Exec which can be used for DAO treasuries.
It checks the next sibling call is `Money::transfer()` and accordingly
verifies the first $n - 1$ output coins match the data set in this
call's auth data.

Additionally we provide a note with the coin params that are verifiably
encrypted to mitigate the attack where Exec is called, but the supplied
`Money::transfer()` call contains an invalid note which cannot be
decrypted by the receiver. In this case, the money would still leave the
DAO treasury but be unspendable.

### Function Params

Define the DAO AuthMoneyTransfer function params
$$ 𝒞_\t{enc} ∈ \t{AuthCoinAttrs}^* $$

Define the DAO $\t{AuthCoinAttrs}$ as
$$ \begin{aligned}
  \t{AuthCoinAttrs}.v &∈ 𝔽ₚ \\
  \t{AuthCoinAttrs}.τ &∈ 𝔽ₚ \\
  \t{AuthCoinAttrs}.ζ &∈ 𝔽ₚ \\
  \t{AuthCoinAttrs}.\t{SH} &∈ 𝔽ₚ \\
  \t{AuthCoinAttrs}.\t{UD} &∈ 𝔽ₚ \\
  \t{AuthCoinAttrs}.\t{EPK} &∈ ℙₚ
\end{aligned} $$
which corresponds to encrypted coin attributes. This provides verifiable
note encryption for all output coins in the sibling `Money::transfer()` call.

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-auth_xfer-params}}
```

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-auth_coinattrs-params}}
```

