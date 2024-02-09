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

# Notes from paper:
# "Efficient Zero-Knowledge Arguments for Arithmetic Circuits in the
#  Discrete Log Setting" by Bootle and others (EUROCRYPT 2016)

from finite_fields import finitefield
import numpy as np

p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
fp = finitefield.IntegersModP(p)

# Number of variables
m = 16
# Number of rows for multiplication statements
n = 3

N = n * m

# Initialize zeroed table
aux = np.full(m, fp(0))

# From the zk-explainer document, we will represent the function:
#
#    def foo(w, a, b):
#        if w:
#            return a * b
#        else:
#            return a + b
#
# Which can be translated mathematically to the statements:
#
#   ab = m
#   w(m - a - b) = v - a - b
#   w^2 = w
#
# Where m is an intermediate value.

var_one = 0
aux[var_one] = fp(1)

var_a = 1
var_b = 2
var_w = 3

aux[var_a] = fp(110)
aux[var_b] = fp(4)
aux[var_w] = fp(1)

# Calculate intermediate advice values
var_m = 4
aux[var_m] = aux[var_a] * aux[var_b]

# Calculate public input values
var_v = 5
aux[var_v] = aux[var_w] * (aux[var_a] * aux[var_b]) + \
    (aux[var_one] - aux[var_w]) * (aux[var_a] + aux[var_b])

# Just a quick enforcement check:
assert aux[var_a] * aux[var_b] == aux[var_m]
assert aux[var_w] * (aux[var_m] - aux[var_a] - aux[var_b]) == \
    aux[var_v] - aux[var_a] - aux[var_b]
assert aux[var_w] * aux[var_w] == aux[var_w]

# Setup the gates. For each row of a, b and c, the statement a b = c holds
# R1CS, more info here:
# http://www.zeroknowledgeblog.com/index.php/the-pinocchio-protocol/r1cs
left = np.full((n, m), fp(0))
right = np.full((n, m), fp(0))
output = np.full((n, m), fp(0))
# ab = m
left[0][var_a] = fp(1)
right[0][var_b] = fp(1)
output[0][var_m] = fp(1)
assert aux.dot(left[0]) * aux.dot(right[0]) == aux.dot(output[0])
# w(m - a - b) = v - a - b
left[1][var_w] = fp(1)
right[1][var_m] = fp(1)
right[1][var_a] = fp(-1)
right[1][var_b] = fp(-1)
output[1][var_v] = fp(1)
output[1][var_a] = fp(-1)
output[1][var_b] = fp(-1)
assert aux.dot(left[1]) * aux.dot(right[1]) == aux.dot(output[1])
# w^2 = w
left[2][var_w] = fp(1)
right[2][var_w] = fp(1)
output[2][var_w] = fp(1)
assert aux.dot(left[2]) * aux.dot(right[2]) == aux.dot(output[2])

