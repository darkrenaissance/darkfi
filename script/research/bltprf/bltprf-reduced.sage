q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
K = GF(q)
a = K(0x00)
b = K(0x05)
E = EllipticCurve(K, (a, b))
G = E(0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000000, 0x02)

p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
assert E.order() == p
Scalar = GF(p)

k = 3
n = 2^k

start_a = [Scalar(110), Scalar(56),  Scalar(89), Scalar(6543),
           Scalar(2),   Scalar(110), Scalar(44), Scalar(78)]

x = Scalar.random_element()
start_b = [x^i for i in range(n)]

start_G = [E.random_element(), E.random_element(), E.random_element(),
           E.random_element(), E.random_element(), E.random_element(),
           E.random_element(), E.random_element()]

assert len(start_a) == len(start_b) == len(start_G) == n

# Dot product
def dot(x, y):
    result = None
    for x_i, y_i in zip(x, y):
        if result is None:
            result = int(x_i) * y_i
        else:
            result += int(x_i) * y_i
    return result

# This is the main commitment we are proving over
P = dot(start_a, start_G)

challenges = []
commits = []

a, G = start_a, start_G

# Iterate k times where n = 2^k
for current_k in range(k, 0, -1):
    # This should make sense to you
    half = 2^(current_k - 1)
    assert half * 2 == len(a)

    a_lo, a_hi = a[:half], a[half:]
    G_lo, G_hi = G[:half], G[half:]

    # L = <a_hi, G_lo>
    # R = <a_lo, G_hi>
    L = dot(a_hi, G_lo)
    R = dot(a_lo, G_hi)
    #z_L = dot(a[half:], b[:half])
    #z_R = dot(a[:half], b[half:])
    commits.append((L, R))

    # Random x value
    challenge = Scalar.random_element()
    challenges.append(challenge)

    # a = a_lo + x^-1 a_hi
    # G = G_lo + x G_hi
    a = [a_lo_i + challenge^-1 * a_hi_i
         for a_lo_i, a_hi_i in zip(a_lo, a_hi)]
    G = [G_lo_i + int(challenge) * G_hi_i
         for G_lo_i, G_hi_i in zip(G_lo, G_hi)]
    assert len(a) == len(G) == half

    # Last iteration
    if current_k == 1:
        assert len(a) == 1
        assert len(G) == 1

        final_a = a[0]
        final_G = G[0]

assert len(challenges) == k

# G_3 = [G1, G2, G3, G4, G5, G6, G7, G8]
# G_2 = [
#     G1 + x G5,
#     G2 + x G6,
#     G3 + x G7,
#     G4 + x G8
# ]
# G_1 = [
#     G_2_1 + x G_2_3,
#     G_2_2 + x G_2_4
# ] = [
#     (G1 + x G5) + x (G3 + x G7) = G1 + x G3 + x G5 + x^2 G7,
#     (G2 + x G6) + x (G4 + x G8) = G2 + x G4 + x G6 + x^2 G8
# ]
#
# We end up with a single remaining value
#
# G_0 = G_1_1 + x G_1_2
#     = G1 + x G2 + x G3 + x^2 G4 + x G5 + x^2 G6 + x^2 G7 + x^3 G8

def get_jth_bit(value, idx):
    digits = bin(value)[2:]
    # Add zero padding
    digits = digits.zfill(k)
    return True if digits[idx] == "1" else False

# get scalar values
counters = []
for i in range(1, n + 1):
    s = Scalar(1)
    for j in range(0, k):
        if get_jth_bit(i - 1, j):
            b = 1
        else:
            b = 0
        s *= challenges[j]^b
    counters.append(s)

assert len(counters) == len(start_G)

# Verifier can recompute the final G value by doing this calc
G_verif = dot(counters, start_G)
assert G_verif == final_G
# final_a value is passed to the verifier

# We can also get this final G value by just looping like we did
# in the proving algo, and recomputing the G values.

# Verification check
L, R = zip(*commits)
challenges_inv = [c^-1 for c in challenges]
assert int(final_a) * G_verif == (P
    + dot(challenges, R) + dot(challenges_inv, L))

