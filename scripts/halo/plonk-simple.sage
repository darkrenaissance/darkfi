F101 = Integers(101)
E = EllipticCurve(F101, [0, 3])
R.<X> = PolynomialRing(F101)
# Extension field of points of 101, and solutions of x^2 + 2
K.<X> = GF(101**2, modulus=X^2 + 2)
# y^2 = x^3 + 3 in this curve defined over the extension field.
# Needed for pairing.
E2 = EllipticCurve(K, [0, 3])
# Generator point because 1^3 + 3 = 4 which is sqrt of 2
G = E([1, 2])
G2 = E2([36, 31*X])
assert G.order() == 17
F17 = Integers(17)
assert F17.square_roots_of_one() == (1, 16)
# 16 == -1
# so now we have the 4th roots of 1
w = vector(F17, [1, 4, -1, -4])

# so now we are still defining our reference string

# we have 4 x labels for our permutation vector but we need 12
# so we generate 2 cosets using quadratic non-residues in F
# all 3 cosets should not share any value in common
k1 = 2
k2 = 3

assert w == vector(F17, [1, 4, 16, 13])
assert k1 * w == vector(F17, [2, 8, 15, 9])
assert k2 * w == vector(F17, [3, 12, 14, 5])

A = matrix(F17, [
    [1,    1,    1,    1],
    [4^0,  4^1,  4^2,  4^3],
    [16^0, 16^1, 16^2, 16^3],
    [13^0, 13^1, 13^2, 13^3]
])
Ai = A.inverse()
P.<x> = F17[]
x = P.0

# We only have 3 gates in this example for x^3 + x = 30

# x     x^2     x^3
# x     x       x
# x^2   x^3     0

# public input: 30

# we have 4 w values so the last column is empty (all set to 0 in this case)

fa = P(list(Ai * vector(F17, [3, 9, 27, 0])))
assert fa(1) == 3
assert fa(4) == 9
assert fa(16) == 27
assert fa(13) == 0
fb = P(list(Ai * vector(F17, [3, 3, 3, 0])))
assert fb(1) == 3
assert fb(4) == 3
assert fb(16) == 3
assert fb(13) == 0
fc = P(list(Ai * vector(F17, [9, 27, 0, 0])))
assert fc(1) == 9
assert fc(4) == 27
assert fc(16) == 0
assert fc(13) == 0

# List of operations
#
# mul, mul, add/cons, null

ql = P(list(Ai * vector(F17, [0, 0, 1, 0])))
qr = P(list(Ai * vector(F17, [0, 0, 1, 0])))
qm = P(list(Ai * vector(F17, [1, 1, 0, 0])))
qo = P(list(Ai * vector(F17, [-1, -1, 0, 0])))
qc = P(list(Ai * vector(F17, [0, 0, -30, 0])))

# permutation/copy constraints

# We are using the coset values here for a, b, c

# 1    4    16   13
# 2    8    15   9
# 3    12   14   5

# Applying the permutation for:

# x     x^2     x^3
# x     x       x
# x^2   x^3     0

# then we get:

# 2    3    12   13
# 1    15   8    9
# 4    16   14   5

# We swap indices whenever there is an equality between wires:

# a1 = b1
# a2 = c1
# ...

sa = P(list(Ai * vector(F17, [2, 3, 12, 13])))
sb = P(list(Ai * vector(F17, [1, 15, 8, 9])))
sc = P(list(Ai * vector(F17, [4, 16, 14, 5])))

# Setup phase complete

# Prove phase

# Round 1

# Create vanishing polynomial which is zero for every root of unity.
# That is Z(w_1) = Z(w_2) = ... = 0
Z = x^4 - 1
assert Z(1) == 0
assert Z(4) == 0
assert Z(16) == 0
assert Z(13) == 0

# 9 random blinding values. We will use:
# 7, 4, 11, 12, 16, 2
# 14, 11, 7 (used in round 2)

# Blind our witness polynomials
# The blinding factors will disappear at the evaluation points.
a = (7*x + 4) * Z + fa
b = (11*x + 12) * Z + fb
c = (16*x + 2) * Z + fc

# During the SRS phase we created a random s point and its powers
s = 2
# So now we evaluate a, b, c with these powers of G
a_s = ZZ(a(s)) * G
b_s = ZZ(b(s)) * G
c_s = ZZ(c(s)) * G

# Round 2

# Random transcript challenges
beta = 12
gamma = 13
# Build accumulation
acc = 1
accs = []
for i in range(4):
    # w_{n + j} corresponds to b(w[i])
    # and w_{2n + j} is c(w[i])
    accs.append(acc)
    acc = acc * (
        (a(w[i]) + beta * w[i] + gamma)
        * (b(w[i]) + beta * k1 * w[i] + gamma)
        * (c(w[i]) + beta * k2 * w[i] + gamma) /
        (
            (a(w[i]) + beta * sa(w[i]) + gamma)
            * (b(w[i]) + beta * sb(w[i]) + gamma)
            * (c(w[i]) + beta * sc(w[i]) + gamma)
        ))
assert accs == [1, 12, 10, 1]
del accs
acc = P(list(Ai * vector(F17, [1, 12, 10, 1])))

Zx = (14*x^2 + 11*x + 7) * Z + acc
# Evaluate z(x) at our secret point
Z_s = ZZ(Zx(s)) * G
print(Z_s)

# Round 3

