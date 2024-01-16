# Contract

<!-- toc -->

Let $\t{PoseidonHash}$ be defined as in the section [PoseidonHash Function](../../crypto-schemes.md#poseidonhash-function).

Let $𝔽ₚ, ℙₚ, \t{DerivePubKey}$ be defined as in the section [Pallas and Vesta](../../crypto-schemes.md#pallas-and-vesta).

Let $\t{PedersenCommit}$ be defined as in the section [Homomorphic Pedersen Commitments](../../crypto-schemes.md#homomorphic-pedersen-commitments).

Let $\t{Params}_\t{DAO}, \t{Bulla}_\t{DAO}, \t{Params}_\t{Proposal}, \t{Bulla}_\t{Proposal}$ be defined as in [DAO Model](model.md).

TODO: add merkle section to crypto-schemes with merklepos, merklepath,
MerkleRoot

TODO: rename slot to block_height

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

Define the DAO propose input function params
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
{{#include ../../../../../src/contract/dao/src/model.rs:dao-propose-input-params}}
```

### Contract Statement

Let $t₀ = \t{CurrentDay} ∈ 𝔽ₚ$ be the current day as defined in [Current Day](model.md#current-day).

Let $\t{Params}_\t{Coin}$ be defined as in [Coin](../money/model.md#coin).

**Valid DAO bulla merkle root** &emsp; check that $R_\t{DAO}$ is a previously
seen merkle root in the DAO contract merkle roots DB.

**Proposal bulla uniqueness** &emsp; whether $𝒫 $ already exists. If yes then fail.

Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  v &∈ 𝔽ₚ \\
  bᵥ &∈ 𝔽ᵥ \\
  b_τ &∈ 𝔽ₚ \\
  p &∈ \t{Params}_\t{Proposal} \\
  p_d &∈ 𝔽ₚ \\
  d &∈ \t{Params}_\t{DAO} \\
  b_d &∈ 𝔽ₚ \\
  (ψ, Π) &∈ \t{MerklePos} × \t{MerklePath} \\
\end{aligned} $$
Attach a proof $π_\t{𝒫 }$ such that the following relations hold:

**Governance token commit** &emsp; export the DAO token ID as an encrypted pedersen
commit $T = \t{PedersenCommit}(d.τ, d.b_τ)$ where $T = ∑_{i ∈ 𝐢} Tᵢ$.

**DAO bulla integrity** &emsp; $ℬ  = \t{Bulla}_\t{DAO}(d, b_d)$

**DAO existence** &emsp; $R_\t{DAO} = \t{MerkleRoot}(ψ, Π, ℬ )$

**Proposal bulla integrity** &emsp; $𝒫 = \t{Bulla}_\t{Proposal}(p, p_d)$
where $p.t₀ = t₀$.

**Proposer limit threshold met** &emsp; check the proposer has supplied enough
inputs that the required funds for the proposer limit set in the DAO is met.
Let the total funds $v = ∑_{i ∈ 𝐢} i.v$, then check $d.L ≤ v$.

**Total funds value commit** &emsp; $V = \t{PedersenCommit}(v, bᵥ)$ where
$V = ∑_{i ∈ 𝐢} i.V$. We use this to check that $v = ∑_{i ∈ 𝐢} i.v$ as
claimed in the *proposer limit threshold met* check.

For each input $i ∈ 𝐢$,

&emsp; **Unused nullifier** &emsp; check that $\cN$ does not exist in the
money contract nullifiers DB.

&emsp; **Valid input coins merkle root** &emsp; check that $i.R_\t{coin}$ is a
previously seen merkle root in the money contract merkle roots DB.

&emsp; Let there be a prover auxiliary witness inputs:
$$ \begin{aligned}
  x_c &∈ 𝔽ₚ \\
  x_σ &∈ 𝔽ₚ \\
  c &∈ \t{Params}_\t{Coin} \\
  bᵥ &∈ 𝔽ᵥ \\
  b_τ &∈ 𝔽ₚ \\
  (ψᵢ, Πᵢ) &∈ \t{MerklePos} × \t{MerklePath} \\
\end{aligned} $$
&emsp; Attach a proof $π_i$ such that the following relations hold:

&emsp; **Nullifier integrity** &emsp; $\cN = \t{PoseidonHash}(x_c, C)$.

&emsp; **Coin value commit** &emsp; $i.V = \t{PedersenCommit}(c.v, bᵥ)$.

&emsp; **Token commit** &emsp; $T = \t{PoseidonHash}(c.τ, b_τ)$.

&emsp; **Valid coin** &emsp; Check $c.P = \t{DerivePubKey}(x_c)$. Let $C = \t{Commit}(c)$. Check $i.R_\t{coin} = \t{MerkleRoot}(ψᵢ, Πᵢ, C)$.

&emsp; **Proof of signature public key ownership** &emsp; $i.\t{PK}_σ = \t{DerivePubKey}(x_σ)$.

