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
# omega = 4, omega^0 = 1, omega^1 = 4, omega^3 = -1 = 13, omega^3 = 13

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

fa = P.lagrange_polynomial(zip([1, 4, 16, 13], [3, 9, 27, 0]))
#fa = P(list(Ai * vector(F17, [3, 9, 27, 0])))
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

# Round 3

alpha = 15

t1Z = a * b * qm + a * ql + b * qr + c * qo + qc

t2Z = ((a + beta * x + gamma)
    * (b + beta * k1 * x + gamma)
    * (c + beta * k2 * x + gamma)) * Zx * alpha

# w[1] is our first root of unity
Zw = Zx(w[1] * x)
t3Z = -((a + beta * sa + gamma)
    * (b + beta * sb + gamma)
    * (c + beta * sc + gamma)) * Zw * alpha

# Lagrangian polynomial which evaluates to 1 at 1
# L_1(w_1) = 1 and 0 on the other evaluation points
L = P(list(Ai * vector(F17, [1, 0, 0, 0])))
assert L(1) == 1
# w_2 = 4
assert L(4) == 0

t4Z = (Zx - 1) * L * alpha^2

tZ = t1Z + t2Z + t3Z + t4Z
# and cancel out the factor Z now
t = P(tZ / Z)

# Split t into 3 parts
# t(X) = t_lo(X) + X^n t_mid(X) + X^{2n} t_hi(X)
t_list = t.list()
t_lo = t_list[0:6]
t_mid = t_list[6:12]
t_hi = t_list[12:18]
# and create the evaluations
t_lo_s = ZZ(P(t_lo)(s)) * G
t_mid_s = ZZ(P(t_mid)(s)) * G
t_hi_s = ZZ(P(t_hi)(s)) * G

# Round 4

zeta = 5

a_ = a(zeta)
b_ = b(zeta)
c_ = c(zeta)
sa_ = sa(zeta)
sb_ = sb(zeta)
t_ = t(zeta)
zw_ = Zx(zeta * w[1])
l_ = L(zeta)
assert a_ == 8
assert b_ == 12
assert c_ == 10
assert sa_ == 0
assert sb_ == 16
assert t_ == 3
assert zw_ == 14

r1 = a_ * b_ * qm + a_ * ql + b_ * qr + c_ * qo + qc

r2 = ((a_ + beta * zeta + gamma)
    * (b_ + beta * k1 * zeta + gamma)
    * (c_ + beta * k2 * zeta + gamma)) * Zx * alpha

r3 = -((a_ + beta * sa_ + gamma)
    * (b_ + beta * sb_ + gamma)
    * beta * zw_ * sc * alpha)

r4 = Zx * l_ * alpha^2

r = r1 + r2 + r3 + r4

r_ = r(zeta)
assert r_ == 7

# Round 5

vega = 12

v1 = P(t_lo)
# Polynomial was in parts consisting of 6 powers
v2 = zeta^6 * P(t_mid)
v3 = zeta^12 * P(t_hi)
v4 = -t_
assert v4 == 14

v5 = (
    vega * (r - r_)
    + vega^2 * (a - a_) + vega^3 * (b - b_) + vega^4 * (c - c_)
    + vega^5 * (sa - sa_) + vega^6 * (sb - sb_)
)

W = v1 + v2 + v3 + v4 + v5
Wz = W / (x - zeta)
# Calculate the opening proof
Wzw = (Zx - zw_) / (x - zeta * w[1])

# Compute evaluations of Wz and Wzw
Wz_s = ZZ(Wz(s)) * G
Wzw_s = ZZ(Wzw(s)) * G

# Finished the proving algo
proof = (a_s, b_s, c_s, Z_s, t_lo_s, t_mid_s, t_hi_s, Wz_s, Wzw_s,
         a_, b_, c_, sa_, sb_, r_, zw_)

# Verification

qm_s = ZZ(qm(s)) * G
ql_s = ZZ(ql(s)) * G
qr_s = ZZ(qr(s)) * G
qo_s = ZZ(qo(s)) * G
qc_s = ZZ(qc(s)) * G
sa_s = ZZ(sa(s)) * G
sb_s = ZZ(sb(s)) * G
sc_s = ZZ(sc(s)) * G

# Check all the points are on the curve.
# y^2 = x^3 + 3
# ...

# Also check the scalar values are in the group for F17
# ...

# step 4: random upsilon
upsilon = 4

# step 5
Z_z = F17(zeta^4 - 1)
assert Z_z == 12

# step 6
# Calculate evaluation of L1 at zeta
L1_z = F17((zeta^4 - 1) / (4 * (zeta - 1)))
assert L1_z == 5

# step 7
# no public inputs in this example

# step 8
t_ = (r_ - (a_ + beta * sa_ + gamma)
           * (b_ + beta * sb_ + gamma)
           * (c_ + gamma) * zw_ * alpha
         - L1_z * alpha^2) / Z_z
assert t_ == 3

# step 9
# qx_s are points, and we are multiplying them by scalars
# so convert the values to integers first
d1 = (ZZ(a_ * b_ * vega) * qm_s
      + ZZ(a_ * vega) * ql_s
      + ZZ(b_ * vega) * qr_s
      + ZZ(c_ * vega) * qo_s
      + vega * qc_s)
d2 = ZZ((a_ + beta * zeta + gamma)
        * (b_ + beta * k1 * zeta + gamma)
        * (c_ + beta * k2 * zeta + gamma)
        * alpha * vega
        + L1_z * alpha^2 * vega
        + F17(upsilon)) * Z_s
d3 = -ZZ((a_ + beta * sa_ + gamma)
         * (b_ + beta * sb_ + gamma)
         * alpha * vega * beta * zw_) * sc_s
d = d1 + d2 + d3

# step 10
f = (t_lo_s + zeta^6 * t_mid_s + zeta^12 * t_hi_s
     + d
     + vega^2 * a_s + vega^3 * b_s + vega^4 * c_s
     + vega^5 * sa_s + vega^6 * sb_s)

# step 11
e = ZZ(t_ + vega * r_
       + vega^2 * a_ + vega^3 * b_ + vega^4 * c_
       + vega^5 * sa_ + vega^6 * sb_
       + upsilon * zw_) * G

# step 12
# construct points for the pairing check
x1 = Wz_s + upsilon * Wzw_s
x2 = s * G2

y1 = zeta * Wz_s + ZZ(upsilon * zeta * w[1]) * Wzw_s + f - e
y2 = G2

# do the pairing check
x1_ = E2(x1)
x2_ = E2(x2)
y1_ = E2(y1)
y2_ = E2(y2)
assert x1_.weil_pairing(x2_, 17) == y1_.weil_pairing(y2_, 17)
