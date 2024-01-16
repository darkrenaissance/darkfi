# Model

Let $\t{Bulla}$ be defined as in the section [Bulla Commitments](../../crypto-schemes.md#bulla-commitments).

Let $ℙₚ, 𝔽ₚ$ be defined as in the section [Pallas and Vesta](../../crypto-schemes.md#pallas-and-vesta).

## Coin

The coin contains the main parameters that define the `Money::transfer()` operation:

* The public key $\t{PK}$ serves a dual role.
  1. Protects receiver privacy from the sender since the corresponding secret
     key is used in the nullifier.
  2. Authorizes the creation of the nullifier by the receiver.
* The core parameters are the value $v$ and the token ID $τ$.
* The serial $ζ$ is randomly selected, and guarantees uniqueness of the coin
  which is used in the nullifier. This simultaneously acts as the coin's random
  blinding factor.
* To enable protocol owned liquidity, we define the spend hook $\t{SH}$
  which adds a constraint that when the coin is spent, it must be called by
  the contract specified. The user data $\t{UD}$ can then be used by the parent
  contract to store additional parameters in the coin. If the parameter length
  exceeds the size of $𝔽ₚ$ then a commit can be used here instead.

Define the coin params
$$ \begin{aligned}
  \t{Params}_\t{Coin}.\t{PK} &∈ ℙₚ \\
  \t{Params}_\t{Coin}.v &∈ ℕ₆₄ \\
  \t{Params}_\t{Coin}.τ &∈ 𝔽ₚ \\
  \t{Params}_\t{Coin}.ζ &∈ 𝔽ₚ \\
  \t{Params}_\t{Coin}.\t{SH} &∈ 𝔽ₚ \\
  \t{Params}_\t{Coin}.\t{UD} &∈ 𝔽ₚ \\
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/money/src/model.rs:coin-attributes}}
```

$$ \t{Coin} : \t{Params}_\t{Coin} → 𝔽ₚ $$
$$ \t{Coin}(p) = \t{Bulla}(\mathcal{X}(p.\t{PK}), \mathcal{Y}(p.\t{PK}), ℕ₆₄2𝔽ₚ(p.v), p.τ, p.ζ, p.\t{SH}, p.\t{UD}) $$

