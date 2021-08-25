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
A1, A2, A3, A4 = var_one, 0, 0, 0
F1, F2, F3, F4 = 1, 0, 0, 0
I = 0
A.append((A1, A2, A3, A4, I))
F.append((F1, F2, F3, F4))

# Row 2
A1, A2, A3, A4 = var_zero, 0, 0, 0
F1, F2, F3, F4 = 0, 1, 0, 0
I = 0
A.append((A1, A2, A3, A4, I))
F.append((F1, F2, F3, F4))

# Row 3
A1, A2, A3, A4 = var_s, var_s, var_zero, 0
F1, F2, F3, F4 = 0, 0, 1, 0
I = 0
A.append((A1, A2, A3, A4, I))
F.append((F1, F2, F3, F4))

# Row 4
A1, A2, A3, A4 = var_s, var_x, var_y, var_sxy
F1, F2, F3, F4 = 0, 0, 0, 1
I = 0
A.append((A1, A2, A3, A4, I))
F.append((F1, F2, F3, F4))

# Row 5
A1, A2, A3, A4 = var_s, var_x, var_y, var_1s_xy
F1, F2, F3, F4 = 0, 0, 1, 0
I = 0
A.append((A1, A2, A3, A4, I))
F.append((F1, F2, F3, F4))

# Row 6
A1, A2, A3, A4 = var_zero, var_sxy, var_1s_xy, 0
F1, F2, F3, F4 = 0, 0, 1, 0
I = var_z
A.append((A1, A2, A3, A4, I))
F.append((F1, F2, F3, F4))

# Row 7
# Empty row
A1, A2, A3, A4 = 0, 0, 0, 0
F1, F2, F3, F4 = 0, 0, 0, 0
I = 0
A.append((A1, A2, A3, A4, I))
F.append((F1, F2, F3, F4))

# Row 8
# Empty row
A1, A2, A3, A4 = 0, 0, 0, 0
F1, F2, F3, F4 = 0, 0, 0, 0
I = 0
A.append((A1, A2, A3, A4, I))
F.append((F1, F2, F3, F4))

for (A1, A2, A3, A4, I), (F1, F2, F3, F4) in zip(A, F):
    assert (F1 * (A1 - 1) + F2 * A1 + F3 * (1 - A1) * (A2 + A3)
            + F4 * (A1 * A2 * A3 - A4) - I) == 0

