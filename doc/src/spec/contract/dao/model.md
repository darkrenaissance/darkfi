# Model

Let $\t{Bulla}$ be defined as in the section [Bulla Commitments](../../crypto-schemes.md#bulla-commitments).

Let $ℙₚ, 𝔽ₚ, \mathcal{X}, \mathcal{Y}, \t{𝔹⁶⁴2𝔽ₚ}$ be defined as in the section [Pallas and Vesta](../../crypto-schemes.md#pallas-and-vesta).

## DAO

The DAO contains the main parameters that define DAO operation:

* The proposer limit $L$ is the minimum number of governance tokens of type
  $τ$ required to create a valid proposal on chain. Note this minimum can
  come from multiple token holders.
* Quorum $Q$ specifies the absolute minimum number of tokens required for
  before a proposal can be accepted.
* The approval ratio $A^\%$ is a tuple that specifies the minimum theshold
  of affirmative yes votes for a proposal to become accepted.
* The public key $PK$ serves a dual role for both encrypted notes, and as
  a key to authorize accepted proposals to be executed.
  This key may be shared widely with all DAO members or within a privileged
  group.

Define the DAO params
$$ \begin{aligned}
  \t{Params}_\t{DAO}.L &∈ ℕ₆₄ \\
  \t{Params}_\t{DAO}.Q &∈ ℕ₆₄ \\
  \t{Params}_\t{DAO}.A^\% &∈ ℕ₆₄ × ℕ₆₄ \\
  \t{Params}_\t{DAO}.τ &∈ 𝔽ₚ \\
  \t{Params}_\t{DAO}.\t{PK} &∈ ℙₚ
\end{aligned} $$
where the approval ratio $\t{Approval}^\% = (q, d)$ defines the equivalence
class $[\frac{q}{d}]$ of fractions defined by $q₁d₂ = q₂d₁ ⟺  [\frac{q₁}{d₁}] \~ [\frac{q₂}{d₂}]$.

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao}}
```

$$ \t{Bulla}_\t{DAO} : \t{Params}_\t{DAO} × 𝔽ₚ → 𝔽ₚ $$
$$ \t{Bulla}_\t{DAO}(p, b_\t{DAO}) = \t{Bulla}(ℕ₆₄2𝔽ₚ(p.L), ℕ₆₄2𝔽ₚ(p.Q), ℕ₆₄2𝔽ₚ(p.A^\%), p.τ, \mathcal{X}(p.\t{PK}), \mathcal{Y}(p.\t{PK}), b_\t{DAO}) $$

## Proposals

### Auth Calls

Let $\t{FuncId}$ be defined as in [Function IDs](../../concepts.md#function-ids).

Let $\t{BLAKE2b}$ be defined as in [BLAKE2b Hash Function](../../crypto-schemes.md#blake2b-hash-function).

Define $\t{AuthCall} = (\t{FuncId}, 𝔹^*)$. Each *authorization call* represents
a child call made by the DAO. The *auth data* field is used by the child invoked
contract to enforce additional invariants.
```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-auth-call}}
```

Define $\t{Commit}_\t{Auth} : \t{AuthCall}^* → 𝔽ₚ$ by
$$ \t{Commit}_{\t{Auth}^*}(c) = 𝔹⁶⁴2𝔽ₚ(\t{BLAKE2b}₆₄(\t{Encode}(c))) $$
which commits to a `Vec<DaoAuthCall>`.

### Proposal

Define the proposal params
$$ \begin{aligned}
  \t{Params}_\t{Proposal}.C &∈ \t{AuthCall}^* \\
  \t{Params}_\t{Proposal}.t₀ &∈ ℕ₆₄ \\
  \t{Params}_\t{Proposal}.D &∈ ℕ₆₄ \\
  \t{Params}_\t{Proposal}.φ &∈ 𝔽ₚ \\
  \t{Params}_\t{Proposal}.\t{DAO} &∈ \t{Bulla}(\t{DAO2𝔽ₚ}(\t{Params}_\t{DAO})) \\
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-proposal}}
```

$$ \t{Bulla}_\t{Proposal} : \t{Params}_\t{Proposal} → 𝔽ₚ⁵ $$
$$ \t{Bulla}_\t{Proposal}(p) = (\t{Commit}_{\t{Auth}^*}(p.C), ℕ₆₄2𝔽ₚ(p.t₀), ℕ₆₄2𝔽ₚ(p.D), p.φ, p.\t{DAO}) $$

## Vote Nullifiers

Additionally for proposals, we keep track of nullifiers for each token weighted
vote for or against a proposal.

Let $\t{PoseidonHash}$ be defined as in the section [PoseidonHash Function](../../crypto-schemes.md#poseidonhash-function).

Let $\mathcal{C}$ be the coin params, and $C$ be the coin commitment
as defined in [Money Contract](TODO).

Let $P$ be a proposal bulla as in the section [Proposal](#proposal).

Define $\t{Nullifier}_\t{Vote} : 𝔽ₚ × 𝔽ₚ × 𝔽ₚ → 𝔽ₚ$ as follows:
$$ \t{Nullifier}_\t{Vote}(\mathcal{C}.s, C, P) = \t{PoseidonHash}(\mathcal{C}.s, C, P) $$

## Blockwindow

Time limits on proposals are expressed in 4 hour windows. Since proofs cannot
guarantee which block they get into, we therefore must modulo the block height
a certain number which we use in the proofs.

```rust
{{#include ../../../../../src/contract/dao/src/lib.rs:dao-blockwindow}}
```

which can be used like this
```rust
{{#include ../../../../../src/contract/dao/src/entrypoint/propose.rs:dao-blockwindow-example-usage}}
```

