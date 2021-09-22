q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
K = GF(q)
a = K(0x00)
b = K(0x05)
E = EllipticCurve(K, (a, b))
G = E(0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000000, 0x02)

p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
assert E.order() == p
F = GF(p)

Poly.<X> = F[]

k = 3
n = 2^k

x = F(88)

px = (F(110) + F(56) * X + F(89) * X^2 + F(6543) * X^3
      + F(2) * X^4 + F(110) * X^5 + F(44) * X^6 + F(78) * X^7)
assert px.degree() <= n
v = px(x)

base_G = [E.random_element(), E.random_element(), E.random_element(),
          E.random_element(), E.random_element(), E.random_element(),
          E.random_element(), E.random_element()]
base_H = E.random_element()
base_U = E.random_element()

# Make the initial commitment to px
blind = F.random_element()
P = int(blind) * base_H + sum(int(k) * G for k, G in zip(px, base_G))

# Dot product
def dot(x, y):
    result = None
    for x_i, y_i in zip(x, y):
        if result is None:
            result = int(x_i) * y_i
        else:
            result += int(x_i) * y_i
    return result

## Step 2
# Sample a random polynomial of degree n - 1
s_poly = Poly([F.random_element() for _ in range(n)])
# Polynomial should evaluate to 0 at x
s_poly -= s_poly(x)
assert s_poly(x) == 0

## Step 3
# Commitment randomness
s_poly_blind = F.random_element()

## Step 4
s_poly_commitment = (int(s_poly_blind) * base_H
                     + sum(int(k) * G for k, G in zip(s_poly, base_G)))

## Step 5
iota = F.random_element()

## Step 8 (following Halo2 not BCSM20 order)
z = F.random_element()

## Step 6
final_poly = s_poly * iota + px
##############################
# This code is not in BCSM20 #
##############################
final_poly -= final_poly(x)
assert final_poly(x) == 0
##############################

## Step 7
blind = s_poly_blind * iota + blind

# Step 8 creation of C' does not happen in Halo2 (see the notes
# from "Comparison to other work")

# Initialize the vectors in step 8
a = list(final_poly)
assert len(a) == n

b = [x^i for i in range(n)]
assert len(b) == len(a)
assert dot(a, b) == final_poly(x)

# Now loop from 3, 2, 1
half_3 = 2^2
assert half_3 * 2 == len(a) == len(b) == len(base_G)

a_lo_4, a_hi_4 = a[:half_3], a[half_3:]
b_lo_4, b_hi_4 = b[:half_3], b[half_3:]
G_lo_4, G_hi_4 = base_G[:half_3], base_G[half_3:]

l_3 = dot(a_hi_4, G_lo_4)
r_3 = dot(a_lo_4, G_hi_4)
value_l_3 = dot(a_hi_4, b_lo_4)
value_r_3 = dot(a_lo_4, b_hi_4)
l_randomness_3 = F.random_element()
r_randomness_3 = F.random_element()
l_3 += (int(value_l_3 * z) * base_U
        + int(l_randomness_3) * base_H)
r_3 += (int(value_r_3 * z) * base_U
        + int(r_randomness_3) * base_H)

challenge_3 = F.random_element()

a_3 = [a_lo_4_i + challenge_3^-1 * a_hi_4_i
       for a_lo_4_i, a_hi_4_i in zip(a_lo_4, a_hi_4)]
b_3 = [b_lo_4_i + challenge_3 * b_hi_4_i
       for b_lo_4_i, b_hi_4_i in zip(b_lo_4, b_hi_4)]
G_3 = [G_lo_4_i + int(challenge_3) * G_hi_4_i
       for G_lo_4_i, G_hi_4_i in zip(G_lo_4, G_hi_4)]

# Not in the paper
blind += l_randomness_3 * challenge_3^-1
blind += r_randomness_3 * challenge_3

# k = 2
half_2 = 2^1
assert half_2 * 2 == len(a_3) == len(b_3) == len(G_3)

a_lo_3, a_hi_3 = a_3[:half_2], a_3[half_2:]
b_lo_3, b_hi_3 = b_3[:half_2], b_3[half_2:]
G_lo_3, G_hi_3 = G_3[:half_2], G_3[half_2:]

l_2 = dot(a_hi_3, G_lo_3)
r_2 = dot(a_lo_3, G_hi_3)
value_l_2 = dot(a_hi_3, b_lo_3)
value_r_2 = dot(a_lo_3, b_hi_3)
l_randomness_2 = F.random_element()
r_randomness_2 = F.random_element()
l_2 += (int(value_l_2 * z) * base_U
        + int(l_randomness_2) * base_H)
r_2 += (int(value_r_2 * z) * base_U
        + int(r_randomness_2) * base_H)

challenge_2 = F.random_element()

a_2 = [a_lo_3_i + challenge_2^-1 * a_hi_3_i
       for a_lo_3_i, a_hi_3_i in zip(a_lo_3, a_hi_3)]
b_2 = [b_lo_3_i + challenge_2 * b_hi_3_i
       for b_lo_3_i, b_hi_3_i in zip(b_lo_3, b_hi_3)]
G_2 = [G_lo_3_i + int(challenge_2) * G_hi_3_i
       for G_lo_3_i, G_hi_3_i in zip(G_lo_3, G_hi_3)]

blind += l_randomness_2 * challenge_2^-1
blind += r_randomness_2 * challenge_2

# k = 1
half_1 = 2^0
assert half_1 * 2 == len(a_2) == len(b_2) == len(G_2)

a_lo_2, a_hi_2 = a_2[:half_1], a_2[half_1:]
b_lo_2, b_hi_2 = b_2[:half_1], b_2[half_1:]
G_lo_2, G_hi_2 = G_2[:half_1], G_2[half_1:]

l_1 = dot(a_hi_2, G_lo_2)
r_1 = dot(a_lo_2, G_hi_2)
value_l_1 = dot(a_hi_2, b_lo_2)
value_r_1 = dot(a_lo_2, b_hi_2)
l_randomness_1 = F.random_element()
r_randomness_1 = F.random_element()
l_1 += (int(value_l_1 * z) * base_U
        + int(l_randomness_1) * base_H)
r_1 += (int(value_r_1 * z) * base_U
        + int(r_randomness_1) * base_H)

challenge_1 = F.random_element()

a_1 = [a_lo_2_i + challenge_1^-1 * a_hi_2_i
       for a_lo_2_i, a_hi_2_i in zip(a_lo_2, a_hi_2)]
b_1 = [b_lo_2_i + challenge_1 * b_hi_2_i
       for b_lo_2_i, b_hi_2_i in zip(b_lo_2, b_hi_2)]
G_1 = [G_lo_2_i + int(challenge_1) * G_hi_2_i
       for G_lo_2_i, G_hi_2_i in zip(G_lo_2, G_hi_2)]

blind += l_randomness_1 * challenge_1^-1
blind += r_randomness_1 * challenge_1

# Finished looping
assert len(a_1) == 1
a = a_1[0]
assert len(G_1) == 1
G = G_1[0]

# Verify

# This is a table of how often the challenges appear in G_1, G_2, ...
# as well as a and b (applies equally)
#
#              12345678
# challenge 3: 00001111
# challenge 2: 00110011
# challenge 1: 01010101
#
s_1 = F(1)
s_2 = challenge_1
s_3 = challenge_2
s_4 = challenge_1 * challenge_2
s_5 = challenge_3
s_6 = challenge_1 * challenge_3
s_7 = challenge_2 * challenge_3
s_8 = challenge_1 * challenge_2 * challenge_3

s = (s_1, s_2, s_3, s_4, s_5, s_6, s_7, s_8)

# Verifier can recompute the final G value by doing this calc
assert G == dot(s, base_G)
assert a == dot([s_i^-1 for s_i in s], list(final_poly))
assert b_1[0] == dot(s, [x^i for i in range(n)])
b = b_1[0]

# Alternatively we have a faster form of calculating b which
# arises naturally from the structure of how it's computed.
#
# b = (1, x, x^2, x^3, x^4, x^5, x^6, x^7)
# i = 3
# b = (     1 + u3 x^4,
#      x   (1 + u3 x^4),
#      x^2 (1 + u3 x^4),
#      x^3 (1 + u3 x^4))
# i = 2
# b = (   1 + u3 x^4 + u2 x^2 (1 + u3 x^4),
#      x (1 + u3 x^4 + u2 x^2 (1 + u3 x^4)))
#   = (  (1 + u2 x^2)(1 + u3 x^4),
#      x (1 + u2 x^2)(1 + u3 x^4))
# i = 1
# b = (1 + u1 x)(1 + u2 x^2)(1 + u3 x^4)
assert ((1 + challenge_1 * x)
        * (1 + challenge_2 * x^2) * (1 + challenge_3 * x^4)) == b

# There are 2 versions of the check below.

# This one is the use_challenges() version
msm = (P - int(v) * base_G[0] + int(iota) * s_poly_commitment
    + int(challenge_1^-1) * l_1 + int(challenge_1) * r_1
    + int(challenge_2^-1) * l_2 + int(challenge_2) * r_2
    + int(challenge_3^-1) * l_3 + int(challenge_3) * r_3)
rhs = int(a) * (G + int(b * z) * base_U) + int(blind) * base_H
assert msm == rhs

# The other version allows the verifier to be a supplied a blinded G value.
# They can substitute this G value into the equaion below, and still verify
# the equation.
# This means construct a valid G value that is used in multiple verifications
# repeatedly.
msm = (P - int(v) * base_G[0] + int(iota) * s_poly_commitment
    + int(challenge_1^-1) * l_1 + int(challenge_1) * r_1
    + int(challenge_2^-1) * l_2 + int(challenge_2) * r_2
    + int(challenge_3^-1) * l_3 + int(challenge_3) * r_3)
rhs = int(a * b * z) * base_U + int(a + blind) * base_H
# compute_g()
# We compute s vector combined challenges.
G = dot(s, base_G)
# H is used for blinding.
G -= base_H
# use_g() version
rhs += int(a) * G
# ... and do the final check
assert msm == rhs

