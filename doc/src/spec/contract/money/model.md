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
* The blinding factor $b$ is randomly selected, and guarantees uniqueness of the coin
  which is used in the nullifier.
* To enable protocol owned liquidity, we define the spend hook $\t{SH}$
  which adds a constraint that when the coin is spent, it must be called by
  the contract specified. The user data $\t{UD}$ can then be used by the parent
  contract to store additional parameters in the coin. If the parameter length
  exceeds the size of $𝔽ₚ$ then a commit can be used here instead.

Define the coin attributes
$$ \begin{aligned}
  \t{Attrs}_\t{Coin}.\t{PK} &∈ ℙₚ \\
  \t{Attrs}_\t{Coin}.v &∈ ℕ₆₄ \\
  \t{Attrs}_\t{Coin}.τ &∈ 𝔽ₚ \\
  \t{Attrs}_\t{Coin}.\t{SH} &∈ 𝔽ₚ \\
  \t{Attrs}_\t{Coin}.\t{UD} &∈ 𝔽ₚ \\
  \t{Attrs}_\t{Coin}.b &∈ 𝔽ₚ \\
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/money/src/model.rs:coin-attributes}}
```

$$ \t{Coin} : \t{Attrs}_\t{Coin} → 𝔽ₚ $$
$$ \t{Coin}(p) = \t{Bulla}(\mathcal{X}(p.\t{PK}), \mathcal{Y}(p.\t{PK}), ℕ₆₄2𝔽ₚ(p.v), p.τ, p.\t{SH}, p.\t{UD}, p.b) $$

## Inputs and Outputs

### Clear Input

Define the clear input attributes
$$ \begin{aligned}
  \t{MoneyClearInput}.v &∈ ℕ₆₄ \\
  \t{MoneyClearInput}.T &∈ ℙₚ \\
  \t{MoneyClearInput}.v_\t{blind} &∈ 𝔽_q \\
  \t{MoneyClearInput}.t_\t{blind} &∈ 𝔽ₚ \\
  \t{MoneyClearInput}.Z &∈ ℙₚ \\
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/money/src/model.rs:money-clear-input}}
```

### Input

Define the input attributes
$$ \begin{aligned}
  \t{MoneyInput}.V &∈ ℙₚ \\
  \t{MoneyInput}.T &∈ 𝔽ₚ \\
  \t{MoneyInput}.N &∈ 𝔽ₚ \\
  \t{MoneyInput}.R &∈ 𝔽ₚ \\
  \t{MoneyInput}.h &∈ 𝔽ₚ \\
  \t{MoneyInput}.U &∈ 𝔽ₚ \\
  \t{MoneyInput}.Z &∈ ℙₚ \\
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/money/src/model.rs:money-input}}
```

### Output

Let $\t{AeadEncNote}$ be defined as in [In-band Secret Distribution](../../crypto-schemes.md#in-band-secret-distribution).

Define the output attributes
$$ \begin{aligned}
  \t{MoneyOutput}.V &∈ ℙₚ \\
  \t{MoneyOutput}.T &∈ 𝔽ₚ \\
  \t{MoneyOutput}.C &∈ 𝔽ₚ \\
  \t{MoneyOutput}.\t{note} &∈ \t{AeadEncNote} \\
\end{aligned} $$

```rust
{{#include ../../../../../src/contract/money/src/model.rs:money-output}}
```

