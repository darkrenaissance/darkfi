# Model

Let $\t{Bulla}$ be defined as in the section [Bulla Commitments][1].

Let $ℙₚ, 𝔽ₚ, \mathcal{X}, \mathcal{Y}, \t{𝔹⁶⁴2𝔽ₚ}$ be defined as in the
section [Pallas and Vesta][2].

Let $\t{Coin}$ be defined as in the section [Coin][3].

## Vesting Configuration

The vesting configuration contains the main parameters that define the
vesting operation:

* The vesting authority public key $VAPK$ controls the vesting
  configuration.
* The vestee public key $VPK$ for withdrawls.
* The shared secret public key $SPK$ controls who can use the vested
  coin.
* Token $t$ is the token type of the to-be-vested input coins.
* Total $T$ is the total amount of the to-be-vested input coins.
* Cliff $C$ is the amount unlocked at the start blockwindow $S$.
* Start $S$ and end $E$ are the blockwindows defining when vesting
  starts and ends.
* Blockwindow value $V$ is the amount unlocked on each blockwindow.

Define the vesting configuration $VC$ params:
$$ \begin{aligned}
  \t{Params}_\t{VC}.\t{VAPK} &∈ ℙₚ \\
  \t{Params}_\t{VC}.\t{VPK} &∈ ℙₚ \\
  \t{Params}_\t{VC}.\t{SPK} &∈ ℙₚ \\
  \t{Params}_\t{VC}.τ &∈ 𝔽ₚ \\
  \t{Params}_\t{VC}.T &∈ ℕ₆₄ \\
  \t{Params}_\t{VC}.C &∈ ℕ₆₄ \\
  \t{Params}_\t{VC}.S &∈ ℕ₆₄ \\
  \t{Params}_\t{VC}.E &∈ ℕ₆₄ \\
  \t{Params}_\t{VC}.V &∈ ℕ₆₄
\end{aligned} $$

```rust
TODO: add model definition path
```

$$ \t{Bulla}_\t{VC} : \t{Params}_\t{VC} × 𝔽ₚ → 𝔽ₚ $$
$$ \begin{aligned}
\t{Bulla}_\t{VC}(p, b_\t{VC}) = \t{Bulla}( \\
\mathcal{X}(p.\t{VAPK}), \mathcal{Y}(p.\t{VAPK}), \\
\mathcal{X}(p.\t{VPK}), \mathcal{Y}(p.\t{VPK}), \\
\mathcal{X}(p.\t{SPK}), \mathcal{Y}(p.\t{SPK}), \\
p.τ, \\
ℕ₆₄2𝔽ₚ(p.T), \\
ℕ₆₄2𝔽ₚ(p.C), \\
ℕ₆₄2𝔽ₚ(p.S), \\
ℕ₆₄2𝔽ₚ(p.E), \\
ℕ₆₄2𝔽ₚ(p.V), \\
b_\t{VC} \\
)
\end{aligned} $$

> Note: Since a vesting configuration bulla is derived using a random
> blinding factor, it's safe to use the same parameters to generate
> different vesting configurations.

## Blockwindow

Time limits on vesting configurations are expressed in 1 day windows.
Since proofs cannot guarantee which block they get into, we therefore
must modulo the block height a certain number which we use in the
proofs.

```rust
TODO: add definition path
```

which can be used like this:
```rust
TODO: add usage example path
```

[1]: ../../crypto-schemes.md#bulla-commitments
[2]: ../../crypto-schemes.md#pallas-and-vesta
[3]: ../money/model.md#coin
