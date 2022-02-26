import numpy as np

q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
K = GF(q)
P.<X> = K[]

# GENERATOR^{2^s} where t * 2^s + 1 = q with t odd.
# In other words, this is a t root of unity.
generator = K(5)
# There is a large 2^32 order subgroup in this curve because it is 2-adic
t = (K(q) - 1) / 2^32
assert int(t) % 2 != 0
delta = generator^(2^32)
assert delta^t == 1

# The size of the multiplicative group is phi(q) = q - 1
# And inside this group are 2 distinct subgroups of size t and 2^s.
# delta is the generator for the size t subgroup, and omega for the 2^s one.
# Taking powers of these generators and multiplying them will produce
# unique cosets that divide the entire group for q.

def get_omega():
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
k = 4
n = 2^k
omega = omega^(2^32 / n)
assert omega^n == 1

# def foo(s, x, y):
#   if s:
#       return x * y
#   else:
#       return x + y

# z = foo(s, x, y)

# Arithmetization for:
# sxy + (s - 1)(x + y) - z = 0
# s(s - 1) = 0

# F1(A1 - 1) + F2 (A1 - I) + F3((1 - A1)(A2 + A3) - A4) + F4(A1 A2 A3 - A4) = 0
A = []
F = []

var_zero = K(0)
var_x = K(4)
var_y = K(6)
var_s = K(1)
var_sxy = var_s * var_x * var_y
var_1s_xy = (1 - var_s) * (var_x + var_y)
# Public
var_z = var_sxy + var_1s_xy

# 4 advice columns
# 4 fixed columns
# 1 instance column

# Row 1
# z = public z
A_1_1, A_2_1, A_3_1, A_4_1 = var_z, 0, 0, 0
F_1_1, F_2_1, F_3_1, F_4_1 = 1, 0, 0, 0
I_1 = var_z

# Row 2
# ~0 == 0
A_1_2, A_2_2, A_3_2, A_4_2 = var_zero, 0, 0, 0
F_1_2, F_2_2, F_3_2, F_4_2 = 0, 1, 0, 0
I_2 = 0

# Row 3
# Boolean check
# (1 - s)(s + 0) == 0
A_1_3, A_2_3, A_3_3, A_4_3 = var_s, var_s, var_zero, var_zero
F_1_3, F_2_3, F_3_3, F_4_3 = 0, 0, 1, 0
I_3 = 0

# Row 4
# s x y == sxy
A_1_4, A_2_4, A_3_4, A_4_4 = var_s, var_x, var_y, var_sxy
F_1_4, F_2_4, F_3_4, F_4_4 = 0, 0, 0, 1
I_4 = 0

# Row 5
# (1 - s)(x + y) = (1-s)(x+y)
A_1_5, A_2_5, A_3_5, A_4_5 = var_s, var_x, var_y, var_1s_xy
F_1_5, F_2_5, F_3_5, F_4_5 = 0, 0, 1, 0
I_5 = 0

# Row 6
# (1 - 0)(sxy + (1-s)(x+y)) = z
A_1_6, A_2_6, A_3_6, A_4_6 = var_zero, var_sxy, var_1s_xy, var_z
F_1_6, F_2_6, F_3_6, F_4_6 = 0, 0, 1, 0
I_6 = 0

A1 = [A_1_1, A_1_2, A_1_3, A_1_4, A_1_5, A_1_6]
A2 = [A_2_1, A_2_2, A_2_3, A_2_4, A_2_5, A_2_6]
A3 = [A_3_1, A_3_2, A_3_3, A_3_4, A_3_5, A_3_6]
A4 = [A_4_1, A_4_2, A_4_3, A_4_4, A_4_5, A_4_6]
F1 = [F_1_1, F_1_2, F_1_3, F_1_4, F_1_5, F_1_6]
F2 = [F_2_1, F_2_2, F_2_3, F_2_4, F_2_5, F_2_6]
F3 = [F_3_1, F_3_2, F_3_3, F_3_4, F_3_5, F_3_6]
F4 = [F_4_1, F_4_2, F_4_3, F_4_4, F_4_5, F_4_6]
I  = [I_1,   I_2,   I_3,   I_4,   I_5,   I_6]

# There should be 5 unused blinding rows.
# see src/plonk/circuit.rs: fn blinding_factors(&self) -> usize;
# We have 9 so we are perfectly fine.

# Add 9 empty rows
assert n - len(A1) == 10
for i in range(10):
    A1.append(K.random_element())
    A2.append(K.random_element())
    A3.append(K.random_element())
    A4.append(K.random_element())
    F1.append(0)
    F2.append(0)
    F3.append(0)
    F4.append(0)
    I.append(K.random_element())

assert (len(A1) == len(A2) == len(A3) == len(A4) == len(F1) == len(F2)
        == len(F3) == len(F4) == len(I) == n)

for A_1_i, A_2_i, A_3_i, A_4_i, F_1_i, F_2_i, F_3_i, F_4_i, I_i in zip(
    A1, A2, A3, A4, F1, F2, F3, F4, I):
    assert (F_1_i * (A_1_i - I_i)
            + F_2_i * A_1_i
            + F_3_i * ((1 - A_1_i) * (A_2_i + A_3_i) - A_4_i)
            + F_4_i * (A_1_i * A_2_i * A_3_i - A_4_i)) == 0

a_1_X = P.lagrange_polynomial((omega^i, A_1_i) for i, A_1_i in enumerate(A1))
a_2_X = P.lagrange_polynomial((omega^i, A_2_i) for i, A_2_i in enumerate(A2))
a_3_X = P.lagrange_polynomial((omega^i, A_3_i) for i, A_3_i in enumerate(A3))
a_4_X = P.lagrange_polynomial((omega^i, A_4_i) for i, A_4_i in enumerate(A4))
f_1_X = P.lagrange_polynomial((omega^i, F_1_i) for i, F_1_i in enumerate(F1))
f_2_X = P.lagrange_polynomial((omega^i, F_2_i) for i, F_2_i in enumerate(F2))
f_3_X = P.lagrange_polynomial((omega^i, F_3_i) for i, F_3_i in enumerate(F3))
f_4_X = P.lagrange_polynomial((omega^i, F_4_i) for i, F_4_i in enumerate(F4))
# Treat the instance wire as a 5th advice wire
a_5_X = P.lagrange_polynomial((omega^i, A_5_i) for i, A_5_i in enumerate(I))

for i, (A_1_i, A_2_i, A_3_i, A_4_i, F_1_i, F_2_i, F_3_i, F_4_i, I_i) in \
    enumerate(zip(A1, A2, A3, A4, F1, F2, F3, F4, I)):
    assert a_1_X(omega^i) == A_1_i
    assert a_2_X(omega^i) == A_2_i
    assert a_3_X(omega^i) == A_3_i
    assert a_4_X(omega^i) == A_4_i
    assert a_5_X(omega^i) == I_i
    assert f_1_X(omega^i) == F_1_i
    assert f_2_X(omega^i) == F_2_i
    assert f_3_X(omega^i) == F_3_i
    assert f_4_X(omega^i) == F_4_i

# beta, gamma
beta = K.random_element()
gamma = K.random_element()

#       0   1   2    3             4           5      ...     15
# A1:   z,  0,  s,   s,            s,          0,
#
#      16  17  18   19            20          21      ...     31
# A2:   -,  -,  s,   x,            x,        sxy,
#
#      32  33  34   35            36          37      ...     47
# A3:   -,  -,  0,   y,            y, (1-s)(x+y),
#
#      48  49  50   51            52          53      ...     63
# A4:   -,  -,  0, sxy, (1-s)(x + y),          z,
#
#      64  65  66   67            68          69      ...     79
# A5:   z,  -,  -,   -,            -,          -,

# z = (0 53 64)
# 0 = (1 5 34 50)
# s = (2 3 4 18)
# x = (19 20)
# sxy = (21 51)
# y = (35 36)
# (1-s)(x+y) = (37 52)

permuted_indices = list(range(n * 5))
assert len(permuted_indices) == 80

# Apply the actual permutation cycles
# z
permuted_indices[0] = 53
permuted_indices[53] = 64
permuted_indices[64] = 0
# ~0
permuted_indices[1] = 5
permuted_indices[5] = 34
permuted_indices[34] = 50
permuted_indices[50] = 1
# s
permuted_indices[2] = 3
permuted_indices[3] = 4
permuted_indices[4] = 18
permuted_indices[18] = 2
# x
permuted_indices[19] = 20
permuted_indices[20] = 19
# sxy
permuted_indices[21] = 51
permuted_indices[51] = 21
# y
permuted_indices[35] = 36
permuted_indices[36] = 35
# (1-s)(x+y)
permuted_indices[37] = 52
permuted_indices[52] = 37

witness = A1 + A2 + A3 + A4 + I
for i, val in enumerate(witness):
    assert val == witness[permuted_indices[i]]

# How to join lists together?
indices = ([omega^i for i in range(n)]
           + [delta * omega^i for i in range(n)]
           + [delta^2 * omega^i for i in range(n)]
           + [delta^3 * omega^i for i in range(n)]
           + [delta^4 * omega^i for i in range(n)])
assert len(indices) == 80
# Permuted indices
sigma_star = [indices[i] for i in permuted_indices]
s = [sigma_star[:n], sigma_star[n:2 * n], sigma_star[2 * n:3 * n],
     sigma_star[3 * n:4 * n], sigma_star[4 * n:]]
assert s[0] + s[1] + s[2] + s[3] + s[4] == sigma_star
v = [A1, A2, A3, A4, I]

# We split the columns into sets of size m.
# Here we will use m = 1 for illustration purposes

# We have 6 usable rows
# n = 16 rows total
# row u (q_last) will be the 7th row
# So we have 9 unusable rows
q_blind = [0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1]
q_last  = [0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]
# Turn both of these into polynomial form
q_blind = P.lagrange_polynomial((omega^i, q_i) for i, q_i in enumerate(q_blind))
assert q_blind(omega^5) == 0
assert q_blind(omega^6) == 0
assert q_blind(omega^7) == 1
assert q_blind(omega^11) == 1
q_last = P.lagrange_polynomial((omega^i, q_i) for i, q_i in enumerate(q_last))
assert q_last(omega^5) == 0
assert q_last(omega^6) == 1
assert q_last(omega^7) == 0
assert q_last(omega^11) == 0

m = 5
assert n == 16
# 6 usable rows
u = 6
# There are 5 columns
# We will split the columns partitions into 5 partitions to make things easy
# So b = 5, and each partition contains only a single column
# We still iterate over the column to make it more obvious
m = 1
permutation_points = [(1, 1)]
last_y_value = 1
ZP = []
# a is the current column partition we are aggregating
for a in range(5):
    # j iterates over the rows
    for j in range(u):
        current = last_y_value

        # i iterates over the columns in our partition
        for i in range(a * m, (a + 1) * m):
            current *= v[i][j] + beta * delta^i * omega^j + gamma
            current /= v[i][j] + beta * s[i][j] + gamma

        last_y_value = current
        permutation_points.append((omega^(j + 1), current))

    ZP_a = P.lagrange_polynomial(permutation_points)
    ZP.append(ZP_a)
    permutation_points = [(1, last_y_value)]

# l_0(X) (1 - ZP,0(X)) = 0
# => ZP,0(1) = 1
assert ZP[0](1) == 1
# Checks for l_0(X) (ZP,a(X) - ZP,a-1(omega^u X)) = 1
# => ZP,a(Z) = ZP,a-1(omega^u X)
# This copies the end value from one partition to the next one
assert ZP[1](omega^0) == ZP[0](omega^u)
assert ZP[2](omega^0) == ZP[1](omega^u)
assert ZP[3](omega^0) == ZP[2](omega^u)
assert ZP[4](omega^0) == ZP[3](omega^u)
# Allow the last value to be either 0 or 1 for full ZK
assert ZP[4](omega^u) in (0, 1)

y = K.random_element()

gate_0 = f_1_X * (a_1_X - a_5_X)
gate_1 = f_2_X * a_1_X
gate_2 = f_3_X * ((1 - a_1_X) * (a_2_X + a_3_X) - a_4_X)
gate_3 = f_4_X * (a_1_X * a_2_X * a_3_X - a_4_X)

c = gate_0 + y * gate_1 + y^2 * gate_2 + y^3 * gate_3
t = X^n - 1
for i in range(n):
    assert c(omega^i) == 0
# Normally we do:
#h = c / t
# But for some reason sage is producing fractional coefficients
h, rem = c.quo_rem(t)
assert rem == 0

# We send commitments to the terms of h(X)
# h_0(x), ..., h_{d - 1}(x)
# Commitments:
# H = [H_0, ..., H_{d - 1}]

x = K.random_element()

# Send evaluations at x of everything we committed to so far
# A_0(x), ..., A_{m - 1}(x)
# ZP,0(x), ..., ZP,b-1(x)
# H_0(x), ..., H_{d-1}(x)
a_evals = [a_1_X(x), a_2_X(x), a_3_X(x), a_4_X(x), a_5_X(x)]

h_evals = []
# Iterate starting from lowest powers first
h_test = 0
for i, h_i in enumerate(h):
    h_evals.append(h_i * x^i)

    h_test += h_i * X^i
assert h_test == h
assert sum(h_evals) == h(x)

assert sum(h_evals) * t(x) == (
    f_1_X(x) * (a_evals[0] - a_evals[4])
    + y * f_2_X(x) * a_evals[0]
    + y^2 * f_3_X(x) * ((1 - a_evals[0]) * (a_evals[1] + a_evals[2])
                        - a_evals[3])
    + y^3 * f_4_X(x) * (a_evals[0] * a_evals[1] * a_evals[2] - a_evals[3]))

