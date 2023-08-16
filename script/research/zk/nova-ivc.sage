#!/usr/bin/env sage

"""
# Resources on Nova

[1] Revisiting the Nova Proof System on a Cycle of Curves
    https://eprint.iacr.org/2023/969.pdf
[2] The zero-knowledge attack of the year might just have happened, or how Nova got broken
    https://www.zksecurity.xyz/blog/posts/nova-attack/
[3] Nova: Recursive Zero-Knowledge Arguments from Folding Schemes
    https://eprint.iacr.org/2021/370.pdf

# Important Notes

* Nova uses a 2-cycle of curves E(Fp) = q, E(Fq) = p
* It also uses a 250 bit hash function, whereby output values can be
  represented in both Fp and Fq.
  See [1] Section 3, paragraph on Hash Functions for a detailed description.

# High Level Description

    z_{i + 1} = F(z_i, aux_i) is one step of the computation.

Our goal is to represent several steps:

    z_{i + 1} = F(...F(F(z_0, aux_0), aux_1)..., aux_i)

We can represent this as i + 1 proofs of the form:

    x₀ = hash(i,     z₀, z_i,       U_i)
    x₁ = hash(i + 1, z₀, z_{i + 1}, U_{i + 1})

as well as a proof that:

    U_{i + 1} = fold(U_i, u_i)

where u_i represents the proof for x₀, x₁ and U_i is the accumulator which
contains the aggregated proofs, as well as the combined witness values.

You can convince yourself that as long as the individual proofs are
valid, you don't need to pass any data along from one proof to another.
Since each proof contains a statement folding the previous accumulator, we
recursively achieve a correct IVC.

In practice the fold is done using pedersen commits so we do that in another
circuit. But since now we have another circuit, we now have relations R1 and R2
that both need to be folded.
See [1] Section 4 Figures 1a and 1b, and Section 5.3 Fig 2.
"""

q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
F1 = GF(q)
E2 = EllipticCurve(F1, (0, 5))

p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
F2 = GF(p)
E1 = EllipticCurve(F2, (0, 5))

# Base field of E1 is F2, and scalar field is F1
# Base field of E2 is F1, and scalar field is F2

assert E1.order() == q
assert E2.order() == p

Gen1 = [E1.random_point() for _ in range(1)]
H1 = E1.random_point()
Gen2 = [E2.random_point() for _ in range(1)]
H2 = E2.random_point()

commit1_blinds = {}
commit2_blinds = {}
def commit_impl(table, F, values, H, Gen):
    if values in table:
        b = table[values]
    else:
        b = F.random_element()
        table[values] = b

    C = b*H
    for v, G in zip(values, Gen):
        assert v.base_ring() == F
        C += v*G
    return C

def commit1(values=()):
    return commit_impl(commit1_blinds, F1, values, H1, Gen1)
def commit2(values=()):
    return commit_impl(commit2_blinds, F2, values, H2, Gen2)

def hash_impl(table, F, E, key):
    if key in table:
        return table[key]

    c = F.random_element()
    while c > 2**250 - 1:
        c = F.random_element()
    table[key] = c
    return c

hash1_table = {}
hash2_table = {}
def hash1(i, z0, z_i, accum_i):
    key = (i, z0, z_i, accum_i)
    return hash_impl(hash1_table, F1, E2, key)
def hash2(i, z0, z_i, accum_i):
    key = (i, z0, z_i, accum_i)
    return hash_impl(hash2_table, F2, E1, key)

# Setup phase for R1CS⁽¹⁾
i = 0

R1_accum0 = (E1(0), F1(0), E1(0), F1(0), F1(0))
R2_accum0 = (E2(0), F2(0), E2(0), F2(0), F2(0))

# We will calculate 5³ as F(z_{i + 1}) = 5 z_i
R1_z0 = F1(1)

# See [1] Section 3 Paragraph "Committed relaxed instances"
# and Section 5.3

# u0 is the initial dummy instance
# Commitment to error vector
R2_u0_E = commit2()
# I think this value is μ in the original Nova paper
R2_u0_s = F2(1)
# Commitment to extended witness
R2_u0_W = commit2()

R2_x0 = F2(hash1(F1(0), R1_z0, R1_z0, R2_accum0))
# We will ignore z on R2, we don't need it
R2_x1 = hash2(F2(0), F2(0), F2(0), R1_accum0)

R2_u0 = (R2_u0_E, R2_u0_s, R2_u0_W, R2_x0, R2_x1)

# This should be extended to all the witness values for the calcs above
R1_witness = (F1(0), R1_z0, R1_z0, R2_accum0, R2_u0, commit2())
# This is weird since R2_u0_s is not in F1, but s is always 1 so it works
R1_w1 = commit1(R1_witness)

R2_accum1 = R2_accum0

# Now we do the actual calc!
R1_z1 = 5*R1_z0

# Remember we said hash values are in both F1 and F2? Now we use that
R1_x0 = F1(R2_x1)
R1_x1 = hash1(F1(1), R1_z0, R1_z1, R2_accum0)

R1_u1_E = commit1()
R1_u1_s = F1(1)
R1_u1 = (R1_u1_E, R1_u1_s, R1_w1, R1_x0, R1_x1)

# ZK proof
# R₁
assert R1_z0 == R1_z0
assert R2_u0_E == commit2()
assert R2_u0_s == F2(1)
assert F1(R2_x0) == hash1(F1(i), R1_z0, R1_z0, R2_accum0)
assert R1_x0 == F1(R2_x1)
assert R1_z1 == 5*R1_z0
assert R1_x1 == hash1(F1(i + 1), R1_z0, R1_z1, R2_accum1)

# Setup phase for R1CS⁽²⁾
# We need to always do the part 2, after doing the part 1 otherwise
# we leave things in an incomplete state.

# We will ignore z on R2, we don't need it
R2_witness = (F2(0), F2(0), F2(0), R1_accum0, R1_u1, commit2())
R2_w1 = commit2(R2_witness)
R1_accum1 = R1_u1

R2_x0 = F2(R1_x1)
R2_x1 = hash2(F2(i+1), F2(0), F2(0), R1_accum1)

R2_u1_E = commit2()
R2_u1_s = F2(1)
R2_u1 = (R2_u1_E, R2_u1_s, R2_w1, R2_x0, R2_x1)

# ZK proof
# R₂
assert R2_u1_E == commit2()
assert R2_u1_s == F2(1)
assert F2(R1_x0) == hash2(F2(0), F2(0), F2(0), R1_accum0)
assert R2_x0 == F2(R1_x1)
assert R2_x1 == hash2(F2(i+1), F2(0), F2(0), R1_accum1)

# Setup phase complete
# Next iteration
i = 1
# Fold(R2_u1, R2_accum1) -> R2_accum2
R2_accum2 = (
    E2.random_point(),      # E
    F2.random_element(),    # s
    E2.random_point(),      # W
    F2.random_element(),    # x0
    F2.random_element()     # x1
)
R1_witness = (F1(i), R1_z0, R1_z1, R2_accum1, R2_u1)
R1_w2 = commit1(R1_witness)

R1_z2 = 5*R1_z1

R1_x0 = F1(R2_x1)
R1_x1 = hash1(F1(i+1), R1_z0, R1_z2, R2_accum2)

R1_u2_E = commit2()
R1_u2_s = F2(1)
R1_u2 = (R1_u2_E, R1_u2_s, R1_w2, R1_x0, R1_x1)

# ZK proof
# R₁
# assert R2_accum2 == Fold(R2_u1, R2_accum1)
assert R2_u1_E == commit2()
assert R2_u1_s == F2(1)
assert F1(R2_x0) == hash1(F1(i), R1_z0, R1_z1, R2_accum1)
assert R1_x0 == F1(R2_x1)
assert R1_z2 == 5*R1_z1
assert R1_x1 == hash1(F1(i+1), R1_z0, R1_z2, R2_accum2)

# Fold(R1_u2, R1_accum1) -> R1_accum2
R1_accum2 = (
    E1.random_point(),      # E
    F1.random_element(),    # s
    E1.random_point(),      # W
    F1.random_element(),    # x0
    F1.random_element()     # x1
)
R2_witness = (F2(i), F2(0), F2(0), R1_accum1, R1_u2)
R2_w2 = commit2(R2_witness)

R2_x0 = F2(R1_x1)
R2_x1 = hash2(F2(i+1), F2(0), F2(0), R1_accum2)

R2_u2_E = commit2()
R2_u2_s = F2(1)
R2_u2 = (R2_u2_E, R2_u2_s, R2_w2, R2_x0, R2_x1)

# ZK proof
# R₂
# assert R1_accum2 == Fold(R1_u1, R1_accum1)
assert R1_u2_E == commit2()
assert R2_u2_s == F2(1)
assert F2(R1_x0) == hash2(F2(i), F2(0), F2(0), R1_accum1)
assert R2_x0 == F2(R1_x1)
assert R2_x1 == hash2(F2(i+1), F2(0), F2(0), R1_accum2)

# Next iteration
i = 2
# Fold(R2_u2, R2_accum2) -> R2_accum3
R2_accum3 = (
    E2.random_point(),      # E
    F2.random_element(),    # s
    E2.random_point(),      # W
    F2.random_element(),    # x0
    F2.random_element()     # x1
)
R1_witness = (F1(i), R1_z0, R1_z2, R2_accum2, R2_u2)
R1_w3 = commit1(R1_witness)

R1_z3 = 5*R1_z2

R1_x0 = F1(R2_x1)
R1_x1 = hash1(F1(i+1), R1_z0, R1_z3, R2_accum3)

R1_u3_E = commit2()
R1_u3_s = F2(1)
R1_u3 = (R1_u3_E, R1_u3_s, R1_w3, R1_x0, R1_x1)

# ZK proof
# R₁
# assert R2_accum3 == Fold(R2_u2, R2_accum2)
assert R2_u2_E == commit2()
assert R2_u2_s == F2(1)
assert F1(R2_x0) == hash1(F1(i), R1_z0, R1_z2, R2_accum2)
assert R1_x0 == F1(R2_x1)
assert R1_z3 == 5*R1_z2
assert R1_x1 == hash1(F1(i+1), R1_z0, R1_z3, R2_accum3)

# Fold(R1_u3, R1_accum2) -> R1_accum3
R1_accum3 = (
    E1.random_point(),      # E
    F1.random_element(),    # s
    E1.random_point(),      # W
    F1.random_element(),    # x0
    F1.random_element()     # x1
)
R2_witness = (F2(i), F2(0), F2(0), R1_accum2, R1_u3)
R2_w3 = commit2(R2_witness)

R2_x0 = F2(R1_x1)
R2_x1 = hash2(F2(i+1), F2(0), F2(0), R1_accum3)

R2_u3_E = commit2()
R2_u3_s = F2(1)
R2_u3 = (R2_u3_E, R2_u3_s, R2_w3, R2_x0, R2_x1)

# ZK proof
# R₂
# assert R1_accum3 == Fold(R1_u3, R1_accum2)
assert R1_u3_E == commit2()
assert R2_u3_s == F2(1)
assert F2(R1_x0) == hash2(F2(i), F2(0), F2(0), R1_accum2)
assert R2_x0 == F2(R1_x1)
assert R2_x1 == hash2(F2(i+1), F2(0), F2(0), R1_accum3)

