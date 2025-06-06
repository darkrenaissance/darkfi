# Key Recovery Scheme

The aim of this scheme is to enable 3 players to generate a single
public key which can be recovered using any $t$ of $n$ players. It is
trustless and anonymous. The scheme can be used for multisig payments
which appear on chain as normal payments.

The basic concept relies on the additivity of functions
$(f∘g)(a) = f(a) + g(a)$, and additive homomorphism of EC points.
That way we avoid heavy MPC multiplications and keep the scheme
lightweight.

The values $x₁, …, xₙ$ are fixed strings known by all players.

Let $⟨x⟩ = \textrm{commit}(x)$ denote a hiding pedersen commitment to
$x$.

## Constructing the Curve

Each player $i$ constructs their own curves, with the resulting curve
being the sum of them all. Given any t points, we can recover the
original curve and hence the secret.

### Player $i$ creates curve $i$

Player $i$ creates a random curve $Cᵢ = Y + a₀ + a₁X + ⋯ + aₜ₋₁Xᵗ⁻¹$,
and broadcasts commits $A₀ = ⟨a₀⟩, …, Aₜ₋₁ = ⟨aₜ₋₁⟩$.

Then player $i$ lifts points
$$ Rⱼ = (xⱼ, yⱼ) ∈ V(Cᵢ) $$
sending each to player $j$.

### Check $Rⱼ ∈ V(Cᵢ)$

Upon receiving player $j$ receiving $Rⱼ$, they check that
$$ ⟨yⱼ⟩ + ⟨a₀⟩ + xⱼ⟨a₁⟩ + ⋯ + xⱼᵗ⁻¹⟨aₜ₋₁⟩ = ∞ $$

## Compute Shared Public Key

Let $C = C₁ + ⋯ + Cₙ$, then the secret key (unknown to any player) is:
$$ d = C(𝟎) $$
The corresponding public key is:
$$ P = A₀₁ + ⋯ + A₀ₙ = ⟨C₁(𝟎) + ⋯ + Cₙ(𝟎)⟩ = ⟨C(𝟎)⟩ $$

## Key Recovery

Let $T ⊆ N$ be the subset $|T| = t$ of players recovering the secret key.
Reordering as needed, all players in $T$ send their points $Rⱼ$ for
curves $C₁, …, Cₙ$ to player 1.

For each curve $Cᵢ$, player 1 now has $t$ points. Using either lagrange
interpolation or row reduction, they can recover curves $C₁, …, Cₙ$ and
compute $C = C₁ + ⋯ + Cₙ$.

Then player 1 computes the shared secret $d = C(𝟎)$.
