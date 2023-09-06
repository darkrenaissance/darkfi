/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

from bls_py import bls12381
from bls_py import pairing
from bls_py import ec
from bls_py.fields import Fq, Fq2, Fq6, Fq12, bls12381_q as Q
import random
import numpy as np

# Section 3.6 from "Why and How zk-SNARK Works"

def rand_scalar():
    return random.randrange(1, bls12381.q)

#x = rand_scalar()
#y = ec.y_for_x(x)

g1 = ec.generator_Fq(bls12381)
g2 = ec.generator_Fq2(bls12381)

null = ec.AffinePoint(Fq(Q, 0), Fq(Q, 1), True, bls12381)
assert g1 + null == g1
null2 = ec.AffinePoint(Fq2.zero(Q), Fq2.zero(Q), True, bls12381)
assert null2 + g2 == g2

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
encrypted_powers_g2 = [
    g2 * (s**i) for i in range(d)
]
encrypted_shifted_powers = [
    g1 * (a * s**i) for i in range(d)
]
encrypted_shifted_powers_g2 = [
    g2 * (a * s**i) for i in range(d)
]

# evaluates unencrypted target polynomial with s: t(s)
target = (s - 1)
# CRS = common reference string = trusted setup parameters
target_crs = g1 * target
alpha_crs = g2 * a
alpha_crs_g1 = g1 * a

# Proving key = (encrypted_powers, encrypted_shifted_powers)
# Verify key = (target_crs, alpha_crs)

# encrypted values of s provided to the prover
# Actual values of s are toxic waste and discarded

#################################
# Prover
#################################

left_poly = np.poly1d([3])
right_poly = np.poly1d([2])
out_poly = np.poly1d([6])

# x^3 - 3x^2 + 2x
main_poly = left_poly * right_poly - out_poly
# (x - 1)
target_poly = np.poly1d([1, -1])

# Calculates polynomial h(x) = p(x) / t(x)
cofactor, remainder = main_poly / target_poly
assert remainder == np.poly1d([0])

# Using encrypted powers and coefficients, evaluates
# E(p(s)) and E(h(s))
def evaluate(poly, encrypted_powers, identity):
    coeffs = list(poly.coef)[::-1]
    result = identity
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
    return result

assert left_poly * right_poly == out_poly

encrypted_left_poly = evaluate(left_poly, encrypted_powers, null)
encrypted_right_poly = evaluate(right_poly, encrypted_powers_g2, null2)
encrypted_out_poly = evaluate(out_poly, encrypted_powers, null)

#assert encrypted_poly == e_p_s
encrypted_cofactor = evaluate(cofactor, encrypted_powers_g2, null2)

# Alpha shifted powers
encrypted_shift_left_poly = evaluate(left_poly, encrypted_shifted_powers, null)
encrypted_shift_right_poly = evaluate(right_poly, encrypted_shifted_powers_g2, null2)
encrypted_shift_out_poly = evaluate(out_poly, encrypted_shifted_powers, null)

# resulting g^p and g^h are provided to the verifier

# proof = (encrypted_poly, encrypted_cofactor, encrypted_shift_poly)

#################################
# Verifier
#################################

# Last check that p = t(s) h

assert pairing.ate_pairing(2 * g1, g2) == pairing.ate_pairing(g1, g2) * pairing.ate_pairing(g1, g2)

# Verify (g^p)^a == g^p'
# Check polynomial restriction:

def check_polynomial_restriction(encrypted_shift_poly, encrypted_poly):
    res1 = pairing.ate_pairing(encrypted_shift_poly, g2)
    res2 = pairing.ate_pairing(encrypted_poly, alpha_crs)
    assert res1 == res2

def check_polynomial_restriction_swapped(encrypted_shift_poly, encrypted_poly):
    res1 = pairing.ate_pairing(g1, encrypted_shift_poly)
    res2 = pairing.ate_pairing(alpha_crs_g1, encrypted_poly)
    assert res1 == res2

check_polynomial_restriction(encrypted_shift_left_poly, encrypted_left_poly)
check_polynomial_restriction_swapped(encrypted_shift_right_poly, encrypted_right_poly)
check_polynomial_restriction(encrypted_shift_out_poly, encrypted_out_poly)

# Valid operation check
# e(g^l, g^r) == e(g^t, g^h) * e(g^o, g)
res1 = pairing.ate_pairing(encrypted_left_poly, encrypted_right_poly)
res2 = pairing.ate_pairing(target_crs, encrypted_cofactor) * \
       pairing.ate_pairing(encrypted_out_poly, g2)
assert res1 == res2

