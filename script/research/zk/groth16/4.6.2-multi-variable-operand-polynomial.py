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
from finite_fields.modp import IntegersModP
from finite_fields.polynomial import polynomialsOver
import random

n = bls12381.n

g1 = ec.generator_Fq(bls12381)
g2 = ec.generator_Fq2(bls12381)

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

l_a_points = [
    (1, 1), (2, 1), (3, 0)
]
l_a = lagrange(l_a_points)
#print(l_a)

l_d_points = [
    (1, 0), (2, 0), (3, 1)
]
l_d = lagrange(l_d_points)
#print(l_d)

# a x b = r_1
# a x c = r_2
# d x c = r_3

# a = 3
# d = 2
L = 3*l_a + 2*l_d
#print(L)

def poly_call(poly, x):
    result = mod_field(0)
    for degree, coeff in enumerate(poly):
        result += coeff * (x**degree)
    return result.n

assert poly_call(L, 1) == 3
assert poly_call(L, 2) == 3
assert poly_call(L, 3) == 2

def rand_scalar():
    return random.randrange(1, bls12381.q)

#################################
# Verifier (trusted setup)
#################################

# samples a random value (a secret)
toxic_scalar = rand_scalar()
# calculate the shift
alpha_shift = rand_scalar()

l_a_s = poly_call(l_a, toxic_scalar)
l_d_s = poly_call(l_d, toxic_scalar)

enc_a_s = g1 * l_a_s
enc_a_s_alpha = enc_a_s * alpha_shift

enc_d_s = g1 * l_d_s
enc_d_s_alpha = enc_d_s * alpha_shift

# Proving key is enc_* values above

# Actual values of s are toxic waste and discarded

verify_key = g2 * alpha_shift

#################################
# Prover
#################################

a = 3
d = 2
assigned_a = enc_a_s * a
assigned_d = enc_d_s * d

assigned_a_shift = enc_a_s_alpha * a
assigned_d_shift = enc_d_s_alpha * d

operand = assigned_a + assigned_d
operand_shift = assigned_a_shift + assigned_d_shift

# proof  = operand, operand_shift

#################################
# Verifier
#################################

e = pairing.ate_pairing
assert e(operand_shift, g2) == e(operand, verify_key)

