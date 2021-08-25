import numpy as np

q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
K = GF(q)
P.<X> = K[]

# GENERATOR^{2^s} where t * 2^s + 1 = q with t odd.
# In other words, this is a t root of unity.
generator = K(5)
# There is a large 2^32 order subgroup in this curve because it is 2-adic
#t = (K(q) - 1) / 2^32
delta = generator^(2^32)

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
k = 3
n = 2^k
omega = omega^(2^32 / n)
assert omega^n == 1

# Arithmetization for:
# sxy + (s - 1)(x + y) - z = 0
# s(s - 1) = 0

# F1(A1 - 1) + F2 A1 + F3(1 - A1)(A2 + A3) + F4(A1 A2 A3 - A4) - I = 0
A = []
F = []

var_one = K(1)
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
A_1_1, A_2_1, A_3_1, A_4_1 = var_one, 0, 0, 0
F_1_1, F_2_1, F_3_1, F_4_1 = 1, 0, 0, 0
I_1 = 0

# Row 2
A_1_2, A_2_2, A_3_2, A_4_2 = var_zero, 0, 0, 0
F_1_2, F_2_2, F_3_2, F_4_2 = 0, 1, 0, 0
I_2 = 0

# Row 3
A_1_3, A_2_3, A_3_3, A_4_3 = var_s, var_s, var_zero, 0
F_1_3, F_2_3, F_3_3, F_4_3 = 0, 0, 1, 0
I_3 = 0

# Row 4
A_1_4, A_2_4, A_3_4, A_4_4 = var_s, var_x, var_y, var_sxy
F_1_4, F_2_4, F_3_4, F_4_4 = 0, 0, 0, 1
I_4 = 0

# Row 5
A_1_5, A_2_5, A_3_5, A_4_5 = var_s, var_x, var_y, var_1s_xy
F_1_5, F_2_5, F_3_5, F_4_5 = 0, 0, 1, 0
I_5 = 0

# Row 6
A_1_6, A_2_6, A_3_6, A_4_6 = var_zero, var_sxy, var_1s_xy, 0
F_1_6, F_2_6, F_3_6, F_4_6 = 0, 0, 1, 0
I_6 = var_z

# Row 7
# Empty row
A_1_7, A_2_7, A_3_7, A_4_7 = 0, 0, 0, 0
F_1_7, F_2_7, F_3_7, F_4_7 = 0, 0, 0, 0
I_7 = 0

# Row 8
# Empty row
A_1_8, A_2_8, A_3_8, A_4_8 = 0, 0, 0, 0
F_1_8, F_2_8, F_3_8, F_4_8 = 0, 0, 0, 0
I_8 = 0

A1 = [A_1_1, A_1_2, A_1_3, A_1_4, A_1_5, A_1_6, A_1_7, A_1_8]
A2 = [A_2_1, A_2_2, A_2_3, A_2_4, A_2_5, A_2_6, A_2_7, A_2_8]
A3 = [A_3_1, A_3_2, A_3_3, A_3_4, A_3_5, A_3_6, A_3_7, A_3_8]
A4 = [A_4_1, A_4_2, A_4_3, A_4_4, A_4_5, A_4_6, A_4_7, A_4_8]
F1 = [F_1_1, F_1_2, F_1_3, F_1_4, F_1_5, F_1_6, F_1_7, F_1_8]
F2 = [F_2_1, F_2_2, F_2_3, F_2_4, F_2_5, F_2_6, F_2_7, F_2_8]
F3 = [F_3_1, F_3_2, F_3_3, F_3_4, F_3_5, F_3_6, F_3_7, F_3_8]
F4 = [F_4_1, F_4_2, F_4_3, F_4_4, F_4_5, F_4_6, F_4_7, F_4_8]
I  = [I_1,   I_2,   I_3,   I_4,   I_5,   I_6,   I_7,   I_8]

for A_1_i, A_2_i, A_3_i, A_4_i, F_1_i, F_2_i, F_3_i, F_4_i, I_i in zip(
    A1, A2, A3, A4, F1, F2, F3, F4, I):
    assert (F_1_i * (A_1_i - 1)
            + F_2_i * A_1_i
            + F_3_i * (1 - A_1_i) * (A_2_i + A_3_i)
            + F_4_i * (A_1_i * A_2_i * A_3_i - A_4_i) - I_i) == 0

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

