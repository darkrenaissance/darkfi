# State

## DAO

Let $ℙₚ, 𝔽ₚ$ be defined as in the section [Pallas and Vesta](../../crypto-schemes.md#pallas-and-vesta).

Define the DAO params
$$ \begin{aligned}
  \t{Params}_\t{DAO}.\t{L} &∈ ℕ₆₄ \\
  \t{Params}_\t{DAO}.\t{Q} &∈ ℕ₆₄ \\
  \t{Params}_\t{DAO}.\t{R}^\% &∈ ℕ₆₄ × ℕ₆₄ \\
  \t{Params}_\t{DAO}.\t{T} &∈ 𝔽ₚ \\
  \t{Params}_\t{DAO}.\t{PK} &∈ ℙₚ
\end{aligned} $$
where the approval ratio $\t{R}^\% = (q, d)$ is defines the equivalence
class $[\frac{q}{d}]$ of fractions defined by $q₁d₂ = q₂d₁ ⟺  [\frac{q₁}{d₁}] \~ [\frac{q₂}{d₂}]$.

```rust
{{#include ../../../../../src/contract/dao/src/model.rs:dao}}
```

