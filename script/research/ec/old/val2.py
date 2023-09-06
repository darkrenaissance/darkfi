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

import itertools
import numpy as np
import pickle

# V: y^2 = x^3 + 4x
# P = (2, 4)
# f = y - 2x

# V = span{1, x, x^2, y, xy, x^2 y}

# multiply polynomials
# perform reduction

K = 11
dim_x = 3
dim_y = 2
n = dim_x*dim_y

P = Px, Py = 2, 4

V = [
    [0, 7, 0, 10, 0, 0],
    [0, 0, 0, 0, 0, 0],
    [1, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0]
]

coeffs = [list(range(K))]*dim_x
x_vals = list(itertools.product(*coeffs))
y_vals = x_vals[:]
ring = list(itertools.product(x_vals, y_vals))
#for idx in range(0, len(ring), 10):
#    print(ring[idx:idx + 10])

def call(poly, point):
    x, y = point
    result = 0
    for j, row in enumerate(poly):
        for i, a in enumerate(row):
            v = a * x**i * y**j
            #print(f'{a} x^{i} y^{j} = {a} {x**i} {y**j} = {v}')
            result += v
    return result % K

p = [[2, 7, 0], [6, 7, 0]]
assert call(p, (2, 4)) == 8
p = [[1, 2, 0], [0, 0, 3]]
assert call(p, (6, 3)) == 7

def sub(poly_a, poly_b):
    c = []
    for row_a, row_b in zip(poly_a, poly_b):
        c_row = [(a - b) % K for (a, b) in zip(row_a, row_b)]
        c.append(c_row)
    return c
def add(poly_a, poly_b):
    c = []
    for row_a, row_b in zip(poly_a, poly_b):
        c_row = [(a + b) % K for (a, b) in zip(row_a, row_b)]
        c.append(c_row)
    return c

a = [[4, 2, 3], [6, 0, 2]]
b = [[2, 1, 2], [9, 3, 4]]
assert sub(a, b) == [[2, 1, 1], [8, 8, 9]]

def _expand_double(poly):
    assert len(poly) > 0
    x_size = len(poly[0])
    return [[0]*2*x_size]*2*len(poly)

def _copy_double(poly):
    assert len(poly) > 0
    x_size = len(poly[0])
    return [row[:] + [0]*x_size for row in poly] + [[0]*2*x_size]*len(poly)

def _mul_x(poly):
    p = []
    for row in poly:
        p.append([0] + row[:-1])
    return p
def _mul_y(p):
    assert len(p) > 0
    x_size = len(p[0])
    return [[0]*x_size] + p[:-1]
def _mul_const(p, c):
    return [[p*c % K for p in row] for row in p]

def mul(a, b):
    result = _expand_double(a)
    for j, row in enumerate(b):
        for i, c in enumerate(row):
            # c x^i y^j
            v = _copy_double(a)
            v = _mul_const(v, c)
            for _ in range(i):
                v = _mul_x(v)
            for _ in range(j):
                v = _mul_y(v)
            result = add(result, v)
    return result

#print(_expand_double(a))
#print(_mul_x(_expand_double(a)))
#print(_mul_y(_expand_double(a)))
#print(_mul_const(_expand_double(a), 2))
a = [[4, 2, 3], [6, 0, 2]]
b = [[2, 1, 2], [9, 3, 4]]
ab = mul(a, b)
assert ab == [
    [8, 8, 5, 7, 6, 0], [4, 3, 10, 8, 5, 0],
    [10, 7, 9, 6, 8, 0], [0, 0, 0, 0, 0, 0]
]

def max_monomial(p):
    degree = 0
    pos = 0, 0
    coeff = 0
    for j, row in enumerate(p):
        for i, c in enumerate(row):
            if c == 0:
                continue
            current_deg = i + j
            if current_deg > degree:
                degree = current_deg
                pos = i, j
                coeff = c
    return coeff, pos
def deg(p):
    _, pos = max_monomial(p)
    return sum(pos)
assert deg([[0, 0, 0], [0, 0, 0]]) == 0
assert deg([[1, 0, 0], [0, 0, 0]]) == 0
assert deg([[0, 1, 0], [0, 0, 0]]) == 1
assert deg([[0, 0, 0], [1, 0, 0]]) == 1
assert deg([[0, 0, 1], [1, 0, 0]]) == 2
assert deg([[0, 0, 0], [0, 0, 1]]) == 3
assert deg([[0, 0, 0], [0, 0, 1], [0, 0, 1]]) == 4

def invert(b):
    n = 11
    (x0, x1, y0, y1) = (1, 0, 0, 1)
    while n != 0:
        q = b // n
        b = n
        n = b % n
        (x0, x1) = (x1, x0 - q * x1)
        (y0, y1) = (y1, y0 - q * y1)
    return b, x0, y0

def _extended_euclid(a, b):
    if a == 0 :
        return 0, 1
             
    x1, y1 = _extended_euclid(b % a, a)
     
    # Update x and y using results of recursive call
    x = y1 - (b//a) * x1
    y = x1
     
    return x, y
def invert(x):
    return _extended_euclid(x, K)[0] % K

assert 5 * invert(5) % K == 1
assert 3 * invert(3) % K == 1
assert 7 * invert(7) % K == 1

def __reduce(p, q):
    q_deg_x, q_deg_y = deg_x(q), deg_y(q)
    r = [row[:] for row in p]
    for j in range(len(r) - 1, -1, -1):
        row = r[j]
        for i in range(len(row)):
            c = row[i]
            if c == 0:
                continue
            term = [[0]*len(row) for _ in r]
            # TODO
    return r

#print(reduce(ab, V))

#ring_valid_denoms = list(filter(lambda g: call(g, P) != 0, ring))
#
#local_ring = []
#for f1 in ring:
#    for g1 in ring_valid_denoms:
#        is_unique = True
#        for (f2, g2) in local_ring:
#            # Test: f1 g2 - f2 g1 in I
#            f1g2 = mul(f1, g2)
#            f2g1 = mul(f2, g1)
#            fg_fg = sub(f1g2, f2g1)

