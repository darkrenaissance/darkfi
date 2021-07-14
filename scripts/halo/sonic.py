# From the Sonic paper

from finite_fields import finitefield
import numpy as np

from multipoly import Variable, MultivariatePolynomial

p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
fp = finitefield.IntegersModP(p)

var_one = fp(1)
var_x = fp(4)
var_y = fp(6)
var_s = fp(1)
var_xy = var_x * var_y
var_sxy = var_s * var_xy
var_1_neg_s = var_one - var_s
var_x_y = var_x + var_y
var_1_s_neg_x_y = var_1_neg_s * var_x_y

a = np.array([
    var_one, var_x, var_xy, var_1_neg_s, var_1_neg_s
])
b = np.array([
    var_one, var_y, var_s, var_x_y, var_s
])
c = np.array([
    var_one, var_xy, var_sxy, var_1_s_neg_x_y, var_s
])
assert len(a) == len(b)
assert len(b) == len(c)

# 1 - s = 1 - s
u1 = np.array([0, 0, 0, 1, -1])
v1 = np.array([0, 0, 0, 0, 0])
w1 = np.array([0, 0, 0, 0, 0])
k1 = 0

assert a.dot(u1) + b.dot(v1) + c.dot(w1) == k1

# xy = xy
u2 = np.array([0, 0, 1, 0, 0])
v2 = np.array([0, 0, 0, 0, 0])
w2 = np.array([0, -1, 0, 0, 0])
k2 = 0

assert a.dot(u2) + b.dot(v2) + c.dot(w2) == k2

# s = s
u3 = np.array([0, 0, 0, 0, 0])
v3 = np.array([0, 0, 1, 0, -1])
w3 = np.array([0, 0, 0, 0, 0])
k3 = 0

assert a.dot(u3) + b.dot(v3) + c.dot(w3) == k3

# s = s
u4 = np.array([0, 0, 0, 0, 0])
v4 = np.array([0, 0, 1, 0, 0])
w4 = np.array([0, 0, 0, 0, -1])
k4 = 0

assert a.dot(u4) + b.dot(v4) + c.dot(w4) == k4

# 1 - s
u5 = np.array([1, 0, 0, -1, 0])
v5 = np.array([0, 0, -1, 0, 0])
w5 = np.array([0, 0, 0, 0, 0])
k5 = 0

assert a.dot(u5) + b.dot(v5) + c.dot(w5) == k5

# x + y
u6 = np.array([0, 1, 0, 0, 0])
v6 = np.array([0, 1, 0, -1, 0])
w6 = np.array([0, 0, 0, 0, 0])
k6 = 0

assert a.dot(u6) + b.dot(v6) + c.dot(w6) == k6

y = Variable("Y")
p = MultivariatePolynomial()
for i, (a_i, b_i, c_i) in enumerate(zip(a, b, c)):
    print(a_i, b_i, c_i)
    p += y**i * (a_i * b_i - c_i)
print(p)

