import numpy as np
from groth_poly_commit import Scalar, poly_commit, create_proof, verify_proof

K = Scalar
# Just use the same finite field we put in the polynomial commitment scheme file
#p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
#K = FiniteField(p)
R.<x, y> = LaurentPolynomialRing(K)

var_one = K(1)
var_x = K(4)
var_y = K(6)
var_s = K(1)
var_xy = var_x * var_y
var_sxy = var_s * var_xy
var_1_neg_s = var_one - var_s
var_x_y = var_x + var_y
var_1_neg_s_x_y = var_1_neg_s * var_x_y
var_s_neg_1 = -var_1_neg_s
var_zero = K(0)

public_v = var_s * (var_x * var_y) + (1 - var_s) * (var_x + var_y)

a = np.array([
    var_one, var_x, var_xy, var_1_neg_s, var_s
])
b = np.array([
    var_one, var_y, var_s, var_x_y, var_s_neg_1
])
c = np.array([
    var_one, var_xy, var_sxy, var_1_neg_s_x_y, var_zero
])
assert len(a) == len(b)
assert len(b) == len(c)

for i, (a_i, b_i, c_i) in enumerate(zip(a, b, c), 1):
    try:
        assert a_i * b_i == c_i
    except AssertionError:
        print("Error for %i" % i)
        raise

# 1 - s = -(s - 1)
u1 = np.array([0, 0, 0, 1, 0])
v1 = np.array([0, 0, 0, 0, 1])
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
u3 = np.array([0, 0, 0, 0, -1])
v3 = np.array([0, 0, 1, 0, 0])
w3 = np.array([0, 0, 0, 0, 0])
k3 = 0

assert a.dot(u3) + b.dot(v3) + c.dot(w3) == k3

# zero = 0
u4 = np.array([0, 0, 0, 0, 0])
v4 = np.array([0, 0, 0, 0, 0])
w4 = np.array([0, 0, 0, 0, 1])
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

# Final check:
# v = s(xy) + (1 - s)(x + y)
u7 = np.array([0, 0, 0, 0, 0])
v7 = np.array([0, 0, 0, 0, 0])
w7 = np.array([0, 0, 1, 1, 0])
k7 = public_v

assert a.dot(u7) + b.dot(v7) + c.dot(w7) == k7

u = np.vstack((u1, u2, u3, u4, u5, u6, u7))
v = np.vstack((v1, v2, v3, v4, v5, v6, v7))
w = np.vstack((w1, w2, w3, w4, w5, w6, w7))
assert u.shape == v.shape
assert u.shape == w.shape

k = np.array((k1, k2, k3, k4, k5, k6, k7))

p = K(0)
for i, (a_i, b_i, c_i) in enumerate(zip(a, b, c), 1):
    #print(a_i, "\t", b_i, "\t", c_i)
    p += y**i * (a_i * b_i - c_i)
print(p)

p = K(0)
for q, (u_q, v_q, w_q, k_q) in enumerate(zip(u, v, w, k)):
    p += y**q * (a.dot(u_q) + b.dot(v_q) + c.dot(w_q) - k_q)
print(p)

n = len(a)
assert len(b) == n
assert len(c) == n

assert u.shape == (7, n)
assert v.shape == u.shape
assert w.shape == u.shape
assert k.shape == (7,)

r_x_y = 0
s_x_y = 0
for i, (a_i, b_i, c_i) in enumerate(zip(a, b, c), 1):
    assert 1 <= i <= n

    r_x_y += x**i * y**i * a_i
    r_x_y += x**-i * y**-i * b_i
    r_x_y += x**(-i - n) * y**(-i - n) * c_i

    u_i = u.T[i - 1]
    v_i = v.T[i - 1]
    w_i = w.T[i - 1]
    u_i_Y = 0
    v_i_Y = 0
    w_i_Y = 0
    for q, (u_q_i, v_q_i, w_q_i) in enumerate(zip(u_i, v_i, w_i), 1):
        assert 1 <= q <= 7

        u_i_Y += y**q * u_q_i
        v_i_Y += y**q * v_q_i
        w_i_Y += y**q * w_q_i

    s_x_y += u_i_Y * x**-i + v_i_Y * x**i + w_i_Y * x**(i + n)

k_y = 0
for q, k_q in enumerate(k, 1):
    assert 1 <= q <= 7
    k_y += y**q * k_q

# Section 6, Figure 2
#
# zkP1
# 4 blinding factors since we evaluate r(X, Y) 3 times
# Blind r(X, Y)
for i in range(1, 4 + 1):
    blind_c_i = K.random_element()
    r_x_y += x**(-2*n - i) * y**(-2*n - i) * blind_c_i

# Commit to r(X, Y)

s_prime_x_y = y**n * s_x_y
for i in range(1, n):
    s_prime_x_y -= (y**i + y**-i) * x**(i + n)

r_x_1 = r_x_y(y=K(1))
t_x_y = r_x_1 * (r_x_y + s_prime_x_y) - y**n * k_y

# This can be opened to r(X, Y) since r(X, Y) = r(XY, 1)
r_x_1_scaled = (r_x_1 * x**(3*n - 1)).univariate_polynomial()
rx1_commit_blind, rx1_commit = poly_commit(r_x_1_scaled)

print("===================")
print(" t(X, Y)")
print("===================")

power_dict = ["⁰", "¹", "²", "³", "⁴", "⁵", "⁶", "⁷", "⁸", "⁹"]

def superscript(number):
    sign = ""
    if number < 0:
        sign = "⁻"
        number = -number
    return sign + "".join([power_dict[int(digit)] for digit in list(str(number))])

decorated = []
for (x_power, y_power), coeff in t_x_y.dict().items():
    if coeff == 1:
        coeff = ""
    display = "%s X%s Y%s" % (coeff, superscript(x_power), superscript(y_power))
    decorated.append([x_power, y_power, display])
decorated.sort(key=lambda x: (x[0], -x[1]))
for _, _, display in decorated:
    print(display)
print()
print("Constant coefficient:", t_x_y.constant_coefficient())
print()

# zkV1
# Send a random y
challenge_y = K.random_element()

# zkP2
# Commit to t(X, y)
t_x = t_x_y(y=challenge_y)
t_x = t_x.univariate_polynomial()
print("===================")
print(" t(X, y)")
print("===================")
print(t_x.dict())
print()
print("Constant coefficient:", t_x.constant_coefficient())

# Split the polynomial into low and hi versions
t_lo_x = 0
t_hi_x = 0
smallest_power = -min(t_x.dict().keys())
for power, coeff in t_x.dict().items():
    assert power != 0
    if power < 0:
        t_lo_x += x**(smallest_power + power) * coeff
    else:
        t_hi_x += x**(power - 1) * coeff
d = t_lo_x.degree() + 1
t_lo_x = t_lo_x.univariate_polynomial()
t_hi_x = t_hi_x.univariate_polynomial()
assert (t_lo_x * x**-d + t_hi_x * x).univariate_polynomial() == t_x

T_lo_commit_blind, T_lo = poly_commit(t_lo_x)
T_hi_commit_blind, T_hi = poly_commit(t_hi_x)

# zkV2
# Send a random z
challenge_z = K.random_element()

# zkP3
# Evaluate a = r(z, 1)
a = r_x_y(x=challenge_z, y=K(1))
# Evaluate b = r(z, y)
b = r_x_y(x=challenge_z, y=challenge_y)
# Evaluate t = t(z, y)
t = t_x_y(x=challenge_z, y=challenge_y)
# Evaluate s = s(z, y)
s = s_prime_x_y(x=challenge_z, y=challenge_y)

# Calculate equivalent openings
# s'(X, Y) is known by both prover and verifier
a_proof = create_proof(r_x_1_scaled, rx1_commit_blind, challenge_z)
assert a_proof.poly_commit == rx1_commit
b_proof = create_proof(r_x_1_scaled, rx1_commit_blind, challenge_y * challenge_z)
assert b_proof.poly_commit == rx1_commit
t_proof_lo = create_proof(t_lo_x, T_lo_commit_blind, challenge_z)
assert t_proof_lo.poly_commit == T_lo
t_proof_hi = create_proof(t_hi_x, T_hi_commit_blind, challenge_z)
assert t_proof_hi.poly_commit == T_hi

# Signature of correct computation not yet implemented
# So just use s for now as is

# Scaling factor
verifier_rescale = challenge_z**(-3*n + 1)
assert a_proof.value * verifier_rescale == a
verifier_rescale = (challenge_y * challenge_z)**(-3*n + 1)
assert b_proof.value * verifier_rescale == b

# zkV3
# Recalculate t from a, b and s
t_new = t_proof_lo.value * challenge_z**-d + t_proof_hi.value * challenge_z
assert t_new == t
t = t_new

k = (y**n * k_y)(y=challenge_y)
t_new = a * (b + s) - k
assert t_new == t
# Verify polynomial commitments

