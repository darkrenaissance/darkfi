# Model

Let $ℙₚ, 𝔽ₚ, \mathcal{X}, \mathcal{Y}$ be defined as in the section [Pallas and Vesta](../../crypto-schemes.md#pallas-and-vesta).

## DAO

The DAO contains the main parameters that define DAO operation:

* The proposer limit $L$ is the minimum number of governance tokens of type
  $T$ required to create a valid proposal on chain. Note this minimum can
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
  \t{Params}_\t{DAO}.T &∈ 𝔽ₚ \\
  \t{Params}_\t{DAO}.PK &∈ ℙₚ
\end{aligned} $$
where the approval ratio $\t{Approval}^\% = (q, d)$ defines the equivalence
class $[\frac{q}{d}]$ of fractions defined by $q₁d₂ = q₂d₁ ⟺  [\frac{q₁}{d₁}] \~ [\frac{q₂}{d₂}]$.

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao}}
```

$$ \t{DAO2𝔽ₚ} : \t{Params}_\t{DAO} → 𝔽ₚ⁷ $$
$$ \t{DAO2𝔽ₚ}(p) = (\t{ℕ₆₄2𝔽ₚ}(p.L), \t{ℕ₆₄2𝔽ₚ}(p.Q), \t{ℕ₆₄2𝔽ₚ}(p.A^\%), p.T, \mathcal{X}(p.PK), \mathcal{Y}(p.PK)) $$

## Proposal

Let $\t{FuncId}$ be defined as in [Function IDs](../../concepts.md#function-ids).

Let $\t{BLAKE3}$ be defined as in [BLAKE3 Hash Function](../../crypto-schemes.md#blake3-hash-function).

Define $\t{AuthCall} = (\t{FuncId}, 𝔹^*)$. Each *authorization call* represents
a child call made by the DAO. The *auth data* field is used by the child invoked
contract to enforce additional invariants.
```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-auth-call}}
```

Define $\t{Commit}_\t{Auth} : \t{AuthCall} → 𝔽ₚ$ by.
$$ \t{Commit}_\t{Auth}(c) = 𝔹³²2𝔽ₚ(\t{BLAKE3}(\t{Encode}(c))) $$

Define the proposal params
$$ \begin{aligned}
  \t{Params}_\t{Proposal}.C &∈ \t{AuthCall}^* \\
  \t{Params}_\t{Proposal}.T₀ &∈ ℕ₆₄ \\
  \t{Params}_\t{Proposal}.D &∈ ℕ₆₄ \\
  \t{Params}_\t{Proposal}.φ &∈ 𝔽ₚ \\
  \t{Params}_\t{Proposal}.\t{DAO} &∈ \t{Bulla}(\t{DAO2𝔽ₚ}(\t{Params}_\t{DAO})) \\
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao-proposal}}
```

## Vote Nullifiers

