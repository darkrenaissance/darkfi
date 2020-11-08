from bls_py import bls12381
from bls_py import pairing
from bls_py import ec
from bls_py.fields import Fq, Fq2, Fq6, Fq12, bls12381_q as Q
from finite_fields.modp import IntegersModP
from finite_fields.polynomial import polynomialsOver
import random

n = bls12381.n

g1 = ec.generator_Fq(bls12381)
g2 = ec.generator_Fq2(bls12381)

null = ec.AffinePoint(Fq(n, 0), Fq(n, 1), True, bls12381)
assert null + g1 == g1
null2 = ec.AffinePoint(Fq2.zero(n), Fq2.zero(n), True, bls12381)
assert null2 + g2 == g2

mod_field = IntegersModP(n)
poly = polynomialsOver(mod_field).factory

def lagrange(points):
    result = poly([0])
    for i, (x_i, y_i) in enumerate(points):
        p = poly([y_i])
        for j, (x_j, y_j) in enumerate(points):
            if i == j:
                continue
            p *= poly([-x_j, 1]) / (x_i - x_j)
        #print(poly)
        #print(poly(1), poly(2), poly(3))
        result += p
    return result

def poly_call(poly, x):
    result = mod_field(0)
    for degree, coeff in enumerate(poly):
        result += coeff * (x**degree)
    return result.n

left_points = [
    (1, 2), (2, 2), (3, 6)
]
left_poly = lagrange(left_points)
#l = poly([2]) * poly([1, -1])
print("Left:")
print(left_poly)
for x, y in left_points:
    assert poly_call(left_poly, x) == y

right_points = [
    (1, 1), (2, 3), (3, 2)
]
right_poly = lagrange(right_points)
print("Right:")
print(right_poly)
for x, y in right_points:
    assert poly_call(right_poly, x) == y

out_points = [
    (1, 2), (2, 6), (3, 12)
]
out_poly = lagrange(out_points)
print("Out:")
print(out_poly)
for x, y in out_points:
    assert poly_call(out_poly, x) == y

target_poly = poly([-1, 1]) * poly([-2, 1]) * poly([-3, 1])
assert poly_call(target_poly, 1) == 0
assert poly_call(target_poly, 2) == 0
assert poly_call(target_poly, 3) == 0

main_poly = left_poly * right_poly - out_poly
cofactor_poly = main_poly / target_poly

assert left_poly * right_poly - out_poly == target_poly * cofactor_poly

def rand_scalar():
    return random.randrange(1, bls12381.q)

#################################
# Verifier (trusted setup)
#################################

# samples a random value (a secret)
toxic_scalar = rand_scalar()
# calculate the shift
alpha_shift = rand_scalar()

# calculates encryptions of s for all powers i in 0 to d
# E(s^i) = g^s^i
degree = 10
enc_s1 = [
    g1 * (toxic_scalar**i) for i in range(degree)
]
enc_s2 = [
    g2 * (toxic_scalar**i) for i in range(degree)
]
enc_s1_shift = [
    g1 * (alpha_shift * toxic_scalar**i) for i in range(degree)
]
enc_s2_shift = [
    g2 * (alpha_shift * toxic_scalar**i) for i in range(degree)
]

# evaluates unencrypted target polynomial with s: t(s)
toxic_target = (toxic_scalar - 1) * (toxic_scalar - 2) * (toxic_scalar - 3)
# CRS = common reference string = trusted setup parameters
target_crs = g1 * toxic_target
alpha_crs = g2 * alpha_shift
alpha_crs_g1 = g1 * alpha_shift

# Proving key = (encrypted_powers, encrypted_shifted_powers)
# Verify key = (target_crs, alpha_crs)

# encrypted values of s provided to the prover
# Actual values of s are toxic waste and discarded

#################################
# Prover
#################################

# Using encrypted powers and coefficients, evaluates
# E(p(s)) and E(h(s))
def evaluate(poly, encrypted_powers, identity):
    result = identity
    for power, coeff in zip(encrypted_powers, poly):
        result += power * coeff.n
    return result

enc_left = evaluate(left_poly, enc_s1, null)
enc_right = evaluate(right_poly, enc_s2, null2)
enc_out = evaluate(out_poly, enc_s1, null)

enc_cofactor = evaluate(cofactor_poly, enc_s2, null2)

# Alpha shifted powers
enc_left_shift = evaluate(left_poly, enc_s1_shift, null)
enc_right_shift = evaluate(right_poly, enc_s2_shift, null2)
enc_out_shift = evaluate(out_poly, enc_s1_shift, null)

#################################
# Verifier
#################################

def restrict_polynomial_g1(encrypted_shift_poly, encrypted_poly):
    res1 = pairing.ate_pairing(encrypted_shift_poly, g2)
    res2 = pairing.ate_pairing(encrypted_poly, alpha_crs)
    assert res1 == res2

def restrict_polynomial_g2(encrypted_shift_poly, encrypted_poly):
    res1 = pairing.ate_pairing(g1, encrypted_shift_poly)
    res2 = pairing.ate_pairing(alpha_crs_g1, encrypted_poly)
    assert res1 == res2

restrict_polynomial_g1(enc_left_shift, enc_left)
restrict_polynomial_g2(enc_right_shift, enc_right)
restrict_polynomial_g1(enc_out_shift, enc_out)

# Valid operation check
# e(g^l, g^r) == e(g^t, g^h) * e(g^o, g)
res1 = pairing.ate_pairing(enc_left, enc_right)
res2 = pairing.ate_pairing(target_crs, enc_cofactor) * \
       pairing.ate_pairing(enc_out, g2)
assert res1 == res2

