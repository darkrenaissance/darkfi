#!/usr/bin/env sage

"""
Implements the simplified Nova scheme introduced in [1] Section 5.1

[1] Nova: Recursive Zero-Knowledge Arguments from Folding Schemes
    https://eprint.iacr.org/2021/370.pdf
[2] Nova: The ZK Bug of the Year (by Wilson Nguyen)
    https://www.youtube.com/watch?v=SOAQCL1NaYY
"""

q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
K = GF(q)

hash_table = {}
def hash(key):
    if key in hash_table:
        return hash_table[key]

    c = K.random_element()
    while c > 2**250 - 1:
        c = K.random_element()
    hash_table[key] = c
    return c

def fold(U, u):
    return U + (u,)

z0 = 5
F = lambda z, ω: 5*z

i = 0
ω0 = ()
z1 = F(z0, ω0)
u1 = hash((1, z0, z1, ()))
U1 = ()
# ZK proof
assert u1 == hash((1, z0, z1, ()))

i = 1
ω1 = ()
U2 = fold(U1, u1)
z2 = F(z1, ω1)
u2 = hash((i+1, z0, z2, U2))
assert u1 == hash((i, z0, z1, U1))
assert U2 == fold(U1, u1)
assert u2 == hash((i+1, z0, z2, U2))

i = 2
ω2 = ()
U3 = fold(U2, u2)
z3 = F(z2, ω2)
u3 = hash((i+1, z0, z3, U3))
assert u2 == hash((i, z0, z2, U2))
assert U3 == fold(U2, u2)
assert u3 == hash((i+1, z0, z3, U3))

# We've now made a proof of what 5^4 is
assert z0 == 5
assert z1 == 5*5
assert z2 == 5*5*5
assert z3 == 5^(i+2)

