#import numpy as np
from groth_poly_commit import Scalar, poly_commit, create_proof, verify_proof

K = Scalar
#R.<x> = LaurentPolynomialRing(K)
R.<x> = PolynomialRing(K)

var_one = K(1)
var_x = K(4)
var_y = K(6)
var_s = K(1)
var_xy = var_x * var_y
var_x_y = var_x + var_y
var_1_neg_s = var_one - var_s
var_sxy = var_s * var_xy
var_1_neg_s_x_y = var_1_neg_s * var_x_y
#var_s_neg_1 = -var_1_neg_s
var_zero = K(0)

public_value = var_s * (var_x * var_y) + (1 - var_s) * (var_x + var_y)

# x * y = xy
a1 = var_x
b1 = var_y
c1 = var_xy
Ql1 = 0
Qr1 = 0
Qm1 = 1
Qo1 = -1
Qc1 = 0
assert Ql1 * a1 + Qr1 * b1 + Qm1 * a1 * b1 + Qo1 * c1 + Qc1 == 0

# x + y = (x + y)
a2 = var_x
b2 = var_y
c2 = var_x_y
Ql2 = 1
Qr2 = 1
Qm2 = 0
Qo2 = -1
Qc2 = 0
assert Ql2 * a2 + Qr2 * b2 + Qm2 * a2 * b2 + Qo2 * c2 + Qc2 == 0

# 1 - s = (1 - s)
a3 = var_one
b3 = var_s
c3 = var_1_neg_s
Ql3 = 1
Qr3 = -1
Qm3 = 0
Qo3 = -1
Qc3 = 0
assert Ql3 * a3 + Qr3 * b3 + Qm3 * a3 * b3 + Qo3 * c3 + Qc3 == 0

# s * (xy) = sxy
a4 = var_s
b4 = var_xy
c4 = var_sxy
Ql4 = 0
Qr4 = 0
Qm4 = 1
Qo4 = -1
Qc4 = 0
assert Ql4 * a4 + Qr4 * b4 + Qm4 * a4 * b4 + Qo4 * c4 + Qc4 == 0

# (1 - s) * (x + y) = [(1 - s)(x + y)]
a5 = var_1_neg_s
b5 = var_x_y
c5 = var_1_neg_s_x_y
Ql5 = 0
Qr5 = 0
Qm5 = 1
Qo5 = -1
Qc5 = 0
assert Ql5 * a5 + Qr5 * b5 + Qm5 * a5 * b5 + Qo5 * c5 + Qc5 == 0

# (sxy) + [(1 - s)(x + y)] = public_value
a6 = var_sxy
b6 = var_1_neg_s_x_y
# Unused
c6 = var_zero

Ql6 = 1
Qr6 = 1
Qm6 = 0
Qo6 = 0
Qc6 = -public_value
assert Ql6 * a6 + Qr6 * b6 + Qm6 * a6 * b6 + Qo6 * c6 + Qc6 == 0

# one == 1
a7 = var_one
# Unused
b7 = var_zero
# Unused
c7 = var_zero

Ql7 = 1
Qr7 = 0
Qm7 = 0
Qo7 = 0
Qc7 = -1
assert Ql7 * a7 + Qr7 * b7 + Qm7 * a7 * b7 + Qo7 * c7 + Qc7 == 0

a = [a1, a2, a3, a4, a5, a6, a7]
b = [b1, b2, b3, b4, b5, b6, b7]
c = [c1, c2, c3, c4, c5, c6, c7]

Ql = [Ql1, Ql2, Ql3, Ql4, Ql5, Ql6]
Qr = [Qr1, Qr2, Qr3, Qr4, Qr5, Qr6]
Qm = [Qm1, Qm2, Qm3, Qm4, Qm5, Qm6]
Qo = [Qo1, Qo2, Qo3, Qo4, Qo5, Qo6]
Qc = [Qc1, Qc2, Qc3, Qc4, Qc5, Qc6]

#    0   1      2      3    4               5               6
# a: x,  x,     1,     s,   1 - s,          sxy,            1
#
#    7   8      9      10   11              12              13
# b: y,  y,     s,     xy,  x + y,          (1 - s)(x + y), -
#
#    14  15     16     17   18              19              20
# c: xy, x + y, 1 - s, sxy, (1 - s)(x + y), -,              -

permuted_indices = [
    1,  0,  6,  9,  16, 17, 2,
    8,  7,  3,  14, 15, 18, 13,
    10, 11, 4,  5,  12, 19, 20
]
eval_domain = range(0, len(permuted_indices))

witness = a + b + c
for i, val in enumerate(a + b + c):
    assert val == witness[permuted_indices[i]]

#def lagrange(domain, codomain):
#    S.<x> = PolynomialRing(K)
#    p = S.lagrange_polynomial(zip(eval_domain, permuted_indices))
#    # Convert to a Laurent polynomial
#    return R(p)

# This is what the prover passes to the verifier
witness_y = R.lagrange_polynomial(enumerate(witness))
assert witness_y(12) == witness[12]

witness_x_a = R.lagrange_polynomial(
    zip(eval_domain[0:7], eval_domain[0:7]))
witness_x_b = R.lagrange_polynomial(
    zip(eval_domain[7:14], eval_domain[7:14]))
witness_x_c = R.lagrange_polynomial(
    zip(eval_domain[14:], eval_domain[14:]))

assert witness_x_a(2) == eval_domain[2]
assert witness_x_b(8) == eval_domain[8]
assert witness_x_c(16) == eval_domain[16]

witness_x_a_prime = R.lagrange_polynomial(
    zip(eval_domain[0:7], permuted_indices[0:7]))
witness_x_b_prime = R.lagrange_polynomial(
    zip(eval_domain[7:14], permuted_indices[7:14]))
witness_x_c_prime = R.lagrange_polynomial(
    zip(eval_domain[14:], permuted_indices[14:]))

assert witness_x_a_prime(2) == permuted_indices[2]
assert witness_x_b_prime(8) == permuted_indices[8]
assert witness_x_c_prime(16) == permuted_indices[16]

v1 = K(2)
v2 = K(3)
px = 1

for i in range(0, len(a)):
    px *= v1 + witness_x_a(i) + v2 * witness_y(i)
for i in range(len(a), 2 * len(a)):
    px *= v1 + witness_x_b(i) + v2 * witness_y(i)
for i in range(2 * len(a), 3 * len(a)):
    px *= v1 + witness_x_c(i) + v2 * witness_y(i)

px_prime = 1

for i in range(0, len(a)):
    px_prime *= v1 + witness_x_a_prime(i) + v2 * witness_y(i)
for i in range(len(a), 2 * len(a)):
    px_prime *= v1 + witness_x_b_prime(i) + v2 * witness_y(i)
for i in range(2 * len(a), 3 * len(a)):
    px_prime *= v1 + witness_x_c_prime(i) + v2 * witness_y(i)

assert px == px_prime

