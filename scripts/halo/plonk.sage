q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
K = GF(q)
P.<X> = K[]

# The pallas and vesta curves are 2-adic. This means there is a large
# power of 2 subgroup within both of their fields.
# This function finds a generator for this subgroup within the field.
def get_omega():
    # Slower alternative:
    #     generator = K.multiplicative_generator()
    # Just hardcode the value here instead
    generator = K(5)
    assert (q - 1) % 2^32 == 0
    # Root of unity
    t = (q - 1) / 2^32
    omega = generator**t

    assert omega != 1
    assert omega^(2^16) != 1
    assert omega^(2^31) != 1
    assert omega^(2^32) == 1

    return omega

# Order of this element is 2^32
omega = get_omega()

# f(s, x, y) = sxy + (1 - s)(x + y)
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

public_value = -(var_s * (var_x * var_y) + (1 - var_s) * (var_x + var_y))

# Ql a + Qr b + Qm a b + Qo c + Qc + P == 0

# See also the file plonk-naive.sage
# x * y = xy
a1, b1, c1 = var_x, var_y, var_xy
Ql1, Qr1, Qm1, Qo1, Qc1 = 0, 0, 1, -1, 0
assert Ql1 * a1 + Qr1 * b1 + Qm1 * a1 * b1 + Qo1 * c1 + Qc1 == 0
# x + y = (x + y)
a2, b2, c2 = var_x, var_y, var_x_y
Ql2, Qr2, Qm2, Qo2, Qc2 = 1, 1, 0, -1, 0
assert Ql2 * a2 + Qr2 * b2 + Qm2 * a2 * b2 + Qo2 * c2 + Qc2 == 0
# 1 - s = (1 - s)
a3, b3, c3 = var_one, var_s, var_1_neg_s
Ql3, Qr3, Qm3, Qo3, Qc3 = 1, -1, 0, -1, 0
assert Ql3 * a3 + Qr3 * b3 + Qm3 * a3 * b3 + Qo3 * c3 + Qc3 == 0
# s * (xy) = sxy
a4, b4, c4 = var_s, var_xy, var_sxy
Ql4, Qr4, Qm4, Qo4, Qc4 = 0, 0, 1, -1, 0
assert Ql4 * a4 + Qr4 * b4 + Qm4 * a4 * b4 + Qo4 * c4 + Qc4 == 0
# (1 - s) * (x + y) = [(1 - s)(x + y)]
a5, b5, c5 = var_1_neg_s, var_x_y, var_1_neg_s_x_y
Ql5, Qr5, Qm5, Qo5, Qc5 = 0, 0, 1, -1, 0
assert Ql5 * a5 + Qr5 * b5 + Qm5 * a5 * b5 + Qo5 * c5 + Qc5 == 0
# (sxy) + [(1 - s)(x + y)] = public_value
# c6 is unused
a6, b6, c6 = var_sxy, var_1_neg_s_x_y, var_zero
Ql6, Qr6, Qm6, Qo6, Qc6 = 1, 1, 0, 0, 0
assert Ql6 * a6 + Qr6 * b6 + Qm6 * a6 * b6 + Qo6 * c6 + Qc6 + public_value == 0
# one == 1, b7 and c7 unused
a7, b7, c7 = var_one, var_zero, var_zero
Ql7, Qr7, Qm7, Qo7, Qc7 = 1, 0, 0, 0, -1
assert Ql7 * a7 + Qr7 * b7 + Qm7 * a7 * b7 + Qo7 * c7 + Qc7 == 0
# Add a last fake constraint so n is a power of 2
# This is needed since we are working with omega whose size is 2^32
# and we will create a generator from it whose order is 2^3
a8, b8, c8 = var_zero, var_zero, var_zero
Ql8, Qr8, Qm8, Qo8, Qc8 = 0, 0, 0, 0, 0
assert Ql8 * a8 + Qr8 * b8 + Qm8 * a8 * b8 + Qo8 * c8 + Qc8 == 0

a = [a1, a2, a3, a4, a5, a6, a7, a8]
b = [b1, b2, b3, b4, b5, b6, b7, b8]
c = [c1, c2, c3, c4, c5, c6, c7, c8]

Ql = [Ql1, Ql2, Ql3, Ql4, Ql5, Ql6, Ql7, Ql8]
Qr = [Qr1, Qr2, Qr3, Qr4, Qr5, Qr6, Qr7, Qr8]
Qm = [Qm1, Qm2, Qm3, Qm4, Qm5, Qm6, Qm7, Qm8]
Qo = [Qo1, Qo2, Qo3, Qo4, Qo5, Qo6, Qo7, Qo8]
Qc = [Qc1, Qc2, Qc3, Qc4, Qc5, Qc6, Qc7, Qc8]

public_values = [0, 0, 0, 0, 0, public_value, 0, 0]

n = 8

for a_i, b_i, c_i, Ql_i, Qr_i, Qm_i, Qo_i, Qc_i, public_i in \
    zip(a, b, c, Ql, Qr, Qm, Qo, Qc, public_values):
    assert (Ql_i * a_i + Qr_i * b_i + Qm_i * a_i * b_i + Qo_i * c_i
            + Qc_i + public_i) == 0

#    0   1      2      3    4               5               6       7
# a: x,  x,     1,     s,   1 - s,          sxy,            1       -
#
#    8   9      10     11   12              13              14      15
# b: y,  y,     s,     xy,  x + y,          (1 - s)(x + y), -       -
#
#    16  17     18     19   20              21              22      23
# c: xy, x + y, 1 - s, sxy, (1 - s)(x + y), -,              -       -

permuted_indices_a = [1,  0,  6,  10, 18, 19, 2,  7]
permuted_indices_b = [8,  9,  3,  16, 17, 20, 14, 15]
permuted_indices_c = [11, 12, 4,  5,  13, 21, 22, 23]
eval_domain = range(0, n * 3)

witness = a + b + c
permuted_indices = permuted_indices_a + permuted_indices_b + permuted_indices_c
for i, val in enumerate(a + b + c):
    assert val == witness[permuted_indices[i]]

omega = omega^(2^32 / n)
assert omega^n == 1

# Calculate the vanishing polynomial
# This is the same as (X - omega^0)(X - omega^1)...(X - omega^{n - 1})
Z_H = X^n - 1
assert Z_H(1) == 0
assert Z_H(omega^4) == 0

qL_X = P.lagrange_polynomial((omega^i, Ql_i) for i, Ql_i in enumerate(Ql))
qR_X = P.lagrange_polynomial((omega^i, Qr_i) for i, Qr_i in enumerate(Qr))
qM_X = P.lagrange_polynomial((omega^i, Qm_i) for i, Qm_i in enumerate(Qm))
qO_X = P.lagrange_polynomial((omega^i, Qo_i) for i, Qo_i in enumerate(Qo))
qC_X = P.lagrange_polynomial((omega^i, Qc_i) for i, Qc_i in enumerate(Qc))

PI_X = P.lagrange_polynomial((omega^i, public_i) for i, public_i
                             in enumerate(public_values))

b_1 = K.random_element()
b_2 = K.random_element()
b_3 = K.random_element()
b_4 = K.random_element()
b_5 = K.random_element()
b_6 = K.random_element()
b_7 = K.random_element()
b_8 = K.random_element()
b_9 = K.random_element()

# Round 1

# Calculate wire witness polynomials
a_X = (b_1 * X + b_2) * Z_H + \
    P.lagrange_polynomial((omega^i, a_i) for i, a_i in enumerate(a))
assert a_X(omega^2) == a[2]
b_X = (b_3 * X + b_4) * Z_H + \
    P.lagrange_polynomial((omega^i, b_i) for i, b_i in enumerate(b))
assert b_X(omega^5) == b[5]
c_X = (b_5 * X + b_6) * Z_H + \
    P.lagrange_polynomial((omega^i, c_i) for i, c_i in enumerate(c))
assert c_X(omega^0) == c[0]

# Commit to a(X), b(X), c(X)

# ...

# Round 2

beta = K.random_element()
gamma = K.random_element()

def find_quadratic_non_residue():
    k = K.random_element()
    while kronecker(k, q) != -1:
        k = K.random_element()
    return k

# These values do not have a square root
k1 = find_quadratic_non_residue()
k2 = find_quadratic_non_residue()
assert k1 != k2

indices = ([omega^i for i in range(n)]
           + [k1 * omega^i for i in range(n)]
           + [k2 * omega^i for i in range(n)])
# Permuted indices
sigma_star = [indices[i] for i in permuted_indices]

permutation_points = [(1, 1)]
for i in range(n - 1):
    x = omega^(i + 1)
    y = 1
    for j in range(i + 1):
        y *= witness[j] + beta * omega^j + gamma
        y *= witness[n + j] + beta * k1 * omega^j + gamma
        y *= witness[2 * n + j] + beta * k2 * omega^j + gamma
        y /= witness[j] + sigma_star[j] * beta + gamma
        y /= witness[n + j] + sigma_star[n + j] * beta + gamma
        y /= witness[2 * n + j] + sigma_star[2 * n + j] * beta + gamma
    permutation_points.append((x, y))

z_X = (b_7 * X^2 + b_8 * X + b_9) * Z_H + \
    P.lagrange_polynomial(permutation_points)

assert witness[0] == 4
assert witness[n] == 6
assert witness[2 * n] == var_xy == 24
assert sigma_star[0] == omega
assert sigma_star[n] == k1 * omega^8
assert sigma_star[2 * n] == k1 * omega^11
assert z_X(omega^0) == 1
assert ((4 + beta + gamma) * (6 + beta * k1 + gamma) * (24 + beta * k2 + gamma)
       ) == (z_X(omega)
        * (4 + omega * beta + gamma)
        * (6 + k1 * omega^8 * beta + gamma)
        * (24 + k1 * omega^11 * beta + gamma))

assert witness[2] == var_one == 1
assert witness[n + 2] == var_s == 1
assert witness[2 * n + 2] == var_1_neg_s == 0
assert sigma_star[2] == omega^6
assert sigma_star[n + 2] == omega^3
assert sigma_star[2 * n + 2] == omega^4
assert (z_X(omega^2) * (1 + beta * omega^2 + gamma) 
        * (1 + beta * k1 * omega^2 + gamma)
        * (0 + beta * k2 * omega^2 + gamma)
       ) == (z_X(omega^3) * (1 + omega^6 * beta + gamma)
        * (1 + omega^3 * beta + gamma)
        * (0 + omega^4 * beta + gamma))


# Round 3

alpha = K.random_element()

Ssigma_1 = P.lagrange_polynomial((omega^i, sigma_star[i]) for i in range(8))
Ssigma_2 = P.lagrange_polynomial((omega^i, sigma_star[n + i]) for i in range(8))
Ssigma_3 = P.lagrange_polynomial((omega^i, sigma_star[2 * n + i])
                                 for i in range(8))
assert Ssigma_1(omega^0) == omega^1
assert Ssigma_1(omega^3) == k1 * omega^10
assert Ssigma_2(omega^2) == omega^3
assert Ssigma_3(omega^7) == k2 * omega^7 == k2 * omega^23

t_X_constraints = ((a_X * b_X * qM_X) + (a_X * qL_X) + (b_X * qR_X)
                   + (c_X * qO_X) + qC_X + PI_X)
for i in range(8):
    assert t_X_constraints(omega^i) == 0

t_X_permutations = ((a_X + beta * X + gamma)
                      * (b_X + beta * k1 * X + gamma)
                      * (c_X + beta * k2 * X + gamma) * z_X
                    # Permutated accumulator
                    - (a_X + beta * Ssigma_1 + gamma)
                      * (b_X + beta * Ssigma_2 + gamma)
                      * (c_X + beta * Ssigma_3 + gamma) * z_X(X * omega))
for i in range(8):
    assert t_X_permutations(omega^i) == 0

L1_X = P.lagrange_polynomial([(1, 1)] + [(omega^i, 0) for i in range(1, n)])
assert L1_X(omega^0) == 1
assert L1_X(omega^2) == 0
t_X_zloops = (z_X - 1) * L1_X
assert t_X_zloops(omega^0) == 0
assert t_X_zloops(omega^2) == 0
assert t_X_zloops(omega^8) == 0

t = (t_X_constraints + t_X_permutations * alpha + t_X_zloops * alpha^2) / Z_H

# Commit to t

# ...

# Round 4

zeta = K.random_element()

a_bar = a_X(zeta)
b_bar = b_X(zeta)
c_bar = c_X(zeta)
s_bar_1 = Ssigma_1(zeta)
s_bar_2 = Ssigma_2(zeta)
z_bar_omega = z_X(zeta * omega)

# Now we provide proofs that all the above values are correct openings
# of the committed polynomials.

# And we prove that a reconstructed version of t(X) from the polynomial
# commitments of the witness and permutation polynomials equals the
# t(X) commitment.
# t(X) - r(X) = 0 where r(X) is the reconstructed polynomial.

# In order to avoid sending Ssigma_1(zeta) and z(zeta), plonk does an
# optimization using the Maller trick documented in section 4 under
# the title "Reducing the number of field elements"

# Round 5

# To reduce the proof by two elements, we construct a linearization polynomial
# which only contains 1 interminate per multiplication expression which is
# enough to prove the polynomial correctly evaluates.

r = (
    # This is proving the constraint polynomial has roots at H
    (a_bar * b_bar * qM_X) + (a_bar * qL_X) + (b_bar * qR_X)
        + (c_bar * qO_X) + PI_X + qC_X

    + alpha * ((a_bar + beta * zeta + gamma)
               * (b_bar + beta * k1 * zeta + gamma)
               * (c_bar + beta * k2 * zeta + gamma) * z_X
               -
               (a_bar + beta * s_bar_1 + gamma)
               * (b_bar + beta * s_bar_2 + gamma)
               * (c_bar + beta * Ssigma_3 + gamma) * z_bar_omega)

    + alpha^2 * (z_X - 1) * L1_X(zeta)

    # t = (t_X_constraints + t_X_permutations * alpha + t_X_zloops * alpha^2)
    #     -------------------------------------------------------------------
    #                                Z_H
    - Z_H(zeta) * t
)

assert r(zeta) == 0

# That is basically the plonk prover. The remaining stuff are details such as
# which polynomial commitment scheme you use (kate, bulletproofs, ...)
