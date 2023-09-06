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

import numpy as np

# Lets prove we know the answer to x**3 + x + 5 == 35 (x = 5)

# We break it down into these statements:

# L1: s1 = x * x
# L2: y = s1 * x
# L3: s2 = y + x
# L4: out = s2 + 5

# Statements are of the form:
# a * b = c

# s1 = x * x
# OR a * b = c, where a = x, b = x and c = s1
L1 = np.array([
   # a  b  c
    [0, 0, 0],  # 1
    [1, 1, 0],  # x
    [0, 0, 0],  # out
    [0, 0, 1],  # s1
    [0, 0, 0],  # y
    [0, 0, 0]   # s2
])

# y = s1 * x
L2 = np.array([
   # a  b  c
    [0, 0, 0],  # 1
    [0, 1, 0],  # x
    [0, 0, 0],  # out
    [1, 0, 0],  # s1
    [0, 0, 1],  # y
    [0, 0, 0]   # s2
])

# s2 = y + x
L3 = np.array([
   # a  b  c
    [0, 1, 0],  # 1
    [1, 0, 0],  # x
    [0, 0, 0],  # out
    [0, 0, 0],  # s1
    [1, 0, 0],  # y
    [0, 0, 1]   # s2
])

# out = s2 + 5
L4 = np.array([
   # a  b  c
    [5, 1, 0],  # 1
    [0, 0, 0],  # x
    [0, 0, 1],  # out
    [0, 0, 0],  # s1
    [0, 0, 0],  # y
    [1, 0, 0]   # s2
])

a = np.array([L.transpose()[0] for L in (L1, L2, L3, L4)])
b = np.array([L.transpose()[1] for L in (L1, L2, L3, L4)])
c = np.array([L.transpose()[2] for L in (L1, L2, L3, L4)])
print("A")
print(a)
print("B")
print(b)
print("C")
print(c)

# The witness
s = np.array([
    1,
    3,
    35,
    9,
    27,
    30
])
print()

#print(s * a * s * b - s * c)
for a_i, b_i, c_i in zip(a, b, c):
    assert sum(s * a_i) * sum(s * b_i) - sum(s * c_i) == 0

print("R1CS done.")
print()

def factorial(x):
    r = 1
    for x_i in range(2, x + 1):
        r *= x_i
    return r

def combinations(n, r):
    return factorial(n) / (factorial(n - r) * factorial(r))

def lagrange(points):
    result = np.poly1d([0])
    for i, (x_i, y_i) in enumerate(points):
        poly = np.poly1d([y_i])
        for j, (x_j, y_j) in enumerate(points):
            if i == j:
                continue
            poly *= np.poly1d([1, -x_j]) / (x_i - x_j)
        #print(poly)
        #print(poly(1), poly(2), poly(3))
        result += poly
    return result

# 1.5, -5.5, 7
#poly = lagrange([(1, 3), (2, 2), (3, 4)])
#print(poly)

def make_qap(a):
    a_qap = []
    a_polys = []
    for a_i in a.transpose():
        poly = lagrange(list(enumerate(a_i, start=1)))
        coeffs = poly.c.tolist()
        if len(coeffs) < 4:
            coeffs = [0] * (4 - len(coeffs)) + coeffs
        a_qap.append(coeffs)
        a_polys.append(poly)
    a_qap = np.array(a_qap)
    print(a_qap)
    return a_polys

print("A")
a_polys = make_qap(a)
print("B")
b_polys = make_qap(b)
print("C")
c_polys = make_qap(c)

def check(polys, x):
    results = []
    for poly in polys:
        results.append(int(poly(x)))
    return results

print()
print("A results at x", check(a_polys, 1))
print()
print("B results at x", check(b_polys, 1))
print()
print("C results at x", check(c_polys, 1))

def combine_polys(polys):
    r = np.poly1d([0])
    for s_i, p_i in zip(s, polys):
        r += s_i * p_i
    return r

print()
print()
A = combine_polys(a_polys)
print("A =")
print(A)
B = combine_polys(b_polys)
print("B =")
print(B)
C = combine_polys(c_polys)
print("C =")
print(C)
print()
t = A * B - C
print("t =")
print(t)

# 4 statements in our R1CS: L1, L2, L3, L4
divisor_poly = np.poly1d([1])
for x in range(1, 4 + 1):
    divisor_poly *= np.poly1d([1, -x])

quot, remainder = np.polydiv(t, divisor_poly)
assert len(remainder.c) == 1
print()
print("Result of QAP:")
print(int(remainder.c[0]))
