# Notation

We use superscript$^*$ to denote an arbitrary length ordered array, usually
corresponding to the `Vec` type in Rust.

$ℕ$ denotes the non-negative integers. $ℕ₆₄$ denotes $ℕ$ restricted to the range
corresponding to `u64` in Rust of $[0, 2⁶⁴)$.

$𝔹$ denotes a single byte $[0, 2⁸)$ corresponding to `u8` in Rust.
We use $𝔹^*$ for an arbitrary sequence of bytes.
Use $ℕ2𝔹ⁿ : ℕ₈ₙ → 𝔹ⁿ$ for converting between the integer $ℕ₈ₙ$ to $𝔹ⁿ$
in little-endian order.

$𝔹ᵃ||𝔹ᵇ$ with $a, b ∈ ℕ$ is used for concatenation of arbitrary bytes.

$ℤ₂$ refers to binary bits.

$\t{im}(f)$ denotes the image of a function $f$.

$[n]$ with $n ∈ ℕ$ denotes the sequences of $n$ values starting with 1.

