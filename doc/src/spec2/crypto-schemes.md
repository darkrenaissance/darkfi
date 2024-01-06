# Cryptographic Schemes

## `PoseidonHash` Function

Poseidon is a circuit friendly permutation hash function described in
the paper GKRRS2019.

| Parameter         | Setting                        |
|-------------------|--------------------------------|
| S-box             | $x → x⁵$                       |
| Full rounds       | 8                              |
| Partial rounds    | 56                             |

Our usage matches that of the halo2 library. Namely using a sponge configuration
with addition which defines the function
$$\textrm{PoseidonHash} : 𝔽ₚ × ⋯ × 𝔽ₚ → 𝔽ₚ$$

## Bulla Commitments

Given an abstract hash function such as [`PoseidonHash`](#poseidonhash-function),
we use a variant of the commit-and-reveal scheme to define anonymized
representations of objects on chain. Contracts then operate with these anonymous
representations which we call bullas.

Let $\textrm{Params} ∈ 𝔽ₚⁿ$ represent object parameters, then we can define
$$ \textrm{Bulla} : 𝔽ₚⁿ × 𝔽ₚ → 𝔽ₚ $$
$$ \textrm{Bulla}(\textrm{Params}, r) = \textrm{PoseidonHash}(\textrm{Params}, r) $$
where $r ∈ 𝔽ₚ$ is a random blinding factor.

Then the bulla (on chain anonymized representation) can be used in contracts
with ZK proofs to construct statements on $\textrm{Params}$.

## Pallas and Vesta

DarkFi uses the elliptic curves Pallas and Vesta that form a 2-cycle.
We denote Pallas by $ₚ$ and Vesta by $ᵥ$. Set the following values:

$$ p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001 $$
$$ q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001 $$

We now construct the base field for each curve $Kₚ$ and $Kᵥ$ as
$Kₚ = 𝔽ₚ$ and $Kᵥ = 𝔽_q$.
Let $f = y² - (x² + 5) ∈ ℤ[x, y]$ be the Weierstrauss normal form of an elliptic curve.
We define $fₚ = f \mod{Kₚ}$ and $fᵥ = f \mod{Kᵥ}$.
Then we instantiate Pallas as $Eₚ = V(fₚ)$ and $Eᵥ = V(fᵥ)$. Now we note the
2-cycle behaviour as

$$ \#V(fₚ) = q $$
$$ \#V(fᵥ) = p $$

An additional projective point at infinity $∞$ is added to the curve.

Let $ℙₚ$ be the group of points with $∞$ on $Eₚ$.

Let $ℙᵥ$ be the group of points with $∞$ on $Eᵥ$.

Arithmetic is mainly done in circuits with $𝔽ₚ$ and $Eₚ$.

