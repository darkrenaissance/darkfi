import numpy as np

# Implementation of Groth09 inner product proof

q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
K = GF(q)
a = K(0x00)
b = K(0x05)
E = EllipticCurve(K, (a, b))
G = E(0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000000, 0x02)

p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
assert E.order() == p
Scalar = GF(p)

x = np.array([
    Scalar(110), Scalar(56), Scalar(89), Scalar(6543), Scalar(2)
])
y = np.array([
    Scalar(4), Scalar(88), Scalar(14), Scalar(33), Scalar(6)
])
z = x.dot(y)

assert len(x) == len(y)

# Create some generator points. Normally we would use hash to curve.
# All these points will be generators since the curve is a cyclic group
H = E.random_element()
G_vec = [E.random_element() for _ in range(len(x))]

# We will now construct a proof

# Commitments

def dot_product(x, y):
    result = None
    for x_i, y_i in zip(x, y):
        if result is None:
            result = int(x_i) * y_i
        else:
            result += int(x_i) * y_i
    return result

t = Scalar.random_element()
r = Scalar.random_element()
s = Scalar.random_element()

C_z = int(t) * H + int(z) * G
C_x = int(r) * H + dot_product(x, G_vec)
C_y = int(s) * H + dot_product(y, G_vec)

d_x = np.array([Scalar.random_element() for _ in range(len(x))])
d_y = np.array([Scalar.random_element() for _ in range(len(x))])
r_d = Scalar.random_element()
s_d = Scalar.random_element()

A_d = int(r_d) * H + dot_product(d_x, G_vec)
B_d = int(s_d) * H + dot_product(d_y, G_vec)

# (cx + d_x)(cy + d_y) = d_x d_y + c(x d_y + y d_x) + c^2 xy
t_0 = Scalar.random_element()
t_1 = Scalar.random_element()

C_0 = int(t_0) * H + int(d_x.dot(d_y)) * G
C_1 = int(t_1) * H + int(x.dot(d_y) + y.dot(d_x)) * G

# Challenge
# Using the Fiat-Shamir transform, we would hash the transcript

c = Scalar.random_element()

# Responses

f_x = c * x + d_x
f_y = c * y + d_y
r_x = c * r + r_d
s_y = c * s + s_d
t_z = c**2 * t + c * t_1 + t_0

# Verify

assert int(c) * C_x + A_d == int(r_x) * H + dot_product(f_x, G_vec)
assert int(c) * C_y + B_d == int(s_y) * H + dot_product(f_y, G_vec)

# Actual inner product check
# Comm(f_x f_y) == e^2 C_z + c Comm(x d_y + y d_x) + Comm(d_x d_y)

assert int(t_z) * H + int(f_x.dot(f_y)) * G == int(c**2) * C_z + int(c) * C_1 + C_0

