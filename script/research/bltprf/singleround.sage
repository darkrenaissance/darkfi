q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
K = GF(q)
a = K(0x00)
b = K(0x05)
E = EllipticCurve(K, (a, b))
G = E(0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000000, 0x02)

p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
assert E.order() == p
Scalar = GF(p)

a1, a2, a3, a4, a5, a6, a7, a8, a9, a10 = (
    Scalar(110), Scalar(56), Scalar(89), Scalar(6543), Scalar(2),
    Scalar(110), Scalar(44), Scalar(78), Scalar(77), Scalar(4))

G1, G2, G3, G4, G5, G6, G7, G8, G9, G10 = (
    E.random_element(), E.random_element(), E.random_element(),
    E.random_element(), E.random_element(), E.random_element(),
    E.random_element(), E.random_element(), E.random_element(),
    E.random_element())

A = (int(a1) * G1 + int(a2) * G2 + int(a3) * G3 + int(a4) * G4
     + int(a5) * G5 + int(a6) * G6 + int(a7) * G7 + int(a8) * G8
     + int(a9) * G9 + int(a10) * G10)

# This function is homomorphic, so:
#
#   H(a_lo_1, a_hi_1) + H(a_lo_2, a_hi_2) = H(a_lo_1 + a_lo_2, a_hi_1 + a_hi_2)
#
# This function is actually the same as the dot product:
#
#   H(a_lo, a_hi) = <a_lo, G_lo> + <a_hi, G_hi>
#
def hash(a_lo, a_hi):
    return (int(a_lo[0]) * G1 + int(a_lo[1]) * G2 + int(a_lo[2]) * G3
            + int(a_lo[3]) * G4 + int(a_lo[4]) * G5 + int(a_hi[0]) * G6
            + int(a_hi[1]) * G7 + int(a_hi[2]) * G8 + int(a_hi[3]) * G9
            + int(a_hi[4]) * G10)

x = Scalar.random_element()

zeros = [Scalar(0)] * 5
a_lo = vector([a1, a2, a3, a4, a5])
a_hi = vector([a6, a7, a8, a9, a10])

L = hash(zeros, a_lo)
R = hash(a_hi, zeros)
P = hash(a_lo, a_hi)

# Same value
assert P == A

a_prime = x * a_lo + x^-1 * a_hi
assert len(a_prime) == 5

# See section 3 of the bulletproofs paper
P_prime = hash(x^-1 * a_prime, x * a_prime)
assert P_prime == int(x^2) * L + P + int(x^-2) * R

# Proof is 5 + 2 elements instead of 10 commitments to each value a_i
proof = (L, R, a)

# Using dot product notation, we can write:
#
# P_prime = <a_prime, G>
#         = <x a_lo + x^-1 a_hi, x^-1 G_lo + x G_hi>
#         = <a_lo, G_lo> + <a_hi, G_hi> + x^2 <a_lo, G_hi> + x^-2 <a_hi, G_lo>
#         = P + x^2 L + x^-2 R
#
# See also P_{k - 1} from:
# https://doc-internal.dalek.rs/bulletproofs/notes/inner_product_proof/index.html
