from bls_py import bls12381
from bls_py import pairing
from bls_py import ec
from bls_py.fields import Fq, Fq2, Fq6, Fq12, bls12381_q as Q
import random
import numpy as np

# Section 3.5 from "Why and How zk-SNARK Works"

def rand_scalar():
    return random.randrange(1, bls12381.q)

#x = rand_scalar()
#y = ec.y_for_x(x)

g1 = ec.generator_Fq(bls12381)
g2 = ec.generator_Fq2(bls12381)

null = ec.AffinePoint(Fq(Q, 0), Fq(Q, 1), True, bls12381)
assert g1 + null == g1

#################################
# Verifier (trusted setup)
#################################

# samples a random value (a secret)
s = rand_scalar()

# calculate the shift
a = rand_scalar()

# calculates encryptions of s for all powers i in 0 to d
# E(s^i) = g^s^i
d = 10
encrypted_powers = [
    g1 * (s**i) for i in range(d)
]
encrypted_shifted_powers = [
    g1 * (a * s**i) for i in range(d)
]

# evaluates unencrypted target polynomial with s: t(s)
target = (s - 1) * (s - 2)

# encrypted values of s provided to the prover
# Actual values of s are toxic waste and discarded

#################################
# Prover
#################################

# delta shift
delta = rand_scalar()

# E(p(s)) = p(s)G
#         = c_d s^d G + ... + c_1 s^1 G + c_0 s^0 G
#         = s^3 G - 3 s^2 G + 2 s G
# E(h(s)) = sG
# t(s) = s^2 - 3s + 2
# E(h(s)) t(s) = s^3 G - 3 s^2 G + 2 s G

# Lets test these manually:

e_s = encrypted_powers
e_p_s = e_s[3] - 3 * e_s[2] + 2 * e_s[1]
e_h_s = e_s[1]
t_s = s**2 - 3*s + 2
# exponentiate with delta
e_p_s *= delta
e_h_s *= delta
assert t_s == target
assert e_p_s == e_h_s * t_s

e_as = encrypted_shifted_powers
e_p_as = e_as[3] - 3 * e_as[2] + 2 * e_as[1]
# exponentiate with delta
e_p_as *= delta
assert e_p_s * a == e_p_as

#############################

# x^3 - 3x^2 + 2x
main_poly = np.poly1d([1, -3, 2, 0])
# (x - 1)(x - 2)
target_poly = np.poly1d([1, -1]) * np.poly1d([1, -2])

# Calculates polynomial h(x) = p(x) / t(x)
cofactor, remainder = main_poly / target_poly
assert remainder == np.poly1d([0])

# Using encrypted powers and coefficients, evaluates
# E(p(s)) and E(h(s))
def evaluate(poly, encrypted_powers):
    coeffs = list(poly.coef)[::-1]
    result = null
    for power, coeff in zip(encrypted_powers, coeffs):
        #print(coeff, power)
        coeff = int(coeff)
        # I have to do this for some strange reason
        # Because if coeff is negative and I do += power * coeff
        # then it gives me a different result than what I expect
        if coeff < 0:
            result -= power * (-coeff)
        else:
            result += power * coeff
    # Add delta to the result
    # Free extra obfuscation to the polynomial
    return result * delta

encrypted_poly = evaluate(main_poly, encrypted_powers)
assert encrypted_poly == e_p_s
encrypted_cofactor = evaluate(cofactor, encrypted_powers)

# Alpha shifted powers
encrypted_shift_poly = evaluate(main_poly, encrypted_shifted_powers)

# resulting g^p and g^h are provided to the verifier

#################################
# Verifier
#################################

# Last check that p = t(s) h

assert encrypted_poly == encrypted_cofactor * target

# Verify (g^p)^a == g^p'

assert encrypted_poly * a == encrypted_shift_poly
