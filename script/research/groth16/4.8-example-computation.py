# Algorithm:
# if w { a * b } else { a + b }

# Equation:
# f(w, a, b) = w(ab) + (1 - w)(a + b) = v

# w(ab) + a + b - w(ab) = v
# w(ab - a - b) = v - a - b

# Constraints:
# 1: [1 a] [1 b] [1 m]
# 2: [1 w] [1 m, -1 a, -1 b] = [1 v, -1 a, -1 b]
# 3: [1 w] [1 w] [1 w]

# f(1, 4, 2) = 8

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

left_variables = {
    "a": lagrange([
        (1, 1), (2, 0), (3, 0)
    ]),
    "w": lagrange([
        (1, 0), (2, 1), (3, 1)
    ])
}

right_variables = {
    "m": lagrange([
        (1, 0), (2, 1), (3, 0)
    ]),
    "a": lagrange([
        (1, 0), (2, -1), (3, 0)
    ]),
    "b": lagrange([
        (1, 1), (2, -1), (3, 0)
    ]),
    "w": lagrange([
        (1, 0), (2, 0), (3, 1)
    ]),
}

out_variables = {
    "m": lagrange([
        (1, 1), (2, 0), (3, 0)
    ]),
    "v": lagrange([
        (1, 0), (2, 1), (3, 0)
    ]),
    "a": lagrange([
        (1, 0), (2, -1), (3, 0)
    ]),
    "b": lagrange([
        (1, 0), (2, -1), (3, 0)
    ]),
    "w": lagrange([
        (1, 0), (2, 0), (3, 1)
    ]),
}

private_inputs = {
    "w": 1,
    "a": 3,
    "b": 2
}

private_inputs["m"] = private_inputs["a"] * private_inputs["b"]
private_inputs["v"] = \
    private_inputs["w"] * (
        private_inputs["m"] - private_inputs["a"] - private_inputs["b"]) \
    + private_inputs["a"] + private_inputs["b"]
assert private_inputs["v"] == 6

left_variable_poly = (
    private_inputs["a"] * left_variables["a"]
    + private_inputs["w"] * left_variables["w"]
)
right_variable_poly = (
    private_inputs["m"] * right_variables["m"]
    + private_inputs["a"] * right_variables["a"]
    + private_inputs["b"] * right_variables["b"]
    + private_inputs["w"] * right_variables["w"]
)
out_variable_poly = (
    private_inputs["m"] * out_variables["m"]
    + private_inputs["v"] * out_variables["v"]
    + private_inputs["a"] * out_variables["a"]
    + private_inputs["b"] * out_variables["b"]
    + private_inputs["w"] * out_variables["w"]
)

# (x - 1)(x - 2)(x - 3)
target_poly = poly([-1, 1]) * poly([-2, 1]) * poly([-3, 1])

def poly_call(poly, x):
    result = mod_field(0)
    for degree, coeff in enumerate(poly):
        result += coeff * (x**degree)
    return result.n

assert poly_call(target_poly, 1) == 0
assert poly_call(target_poly, 2) == 0
assert poly_call(target_poly, 3) == 0

main_poly = left_variable_poly * right_variable_poly - out_variable_poly
cofactor_poly = main_poly / target_poly

assert (
    left_variable_poly * right_variable_poly == \
    cofactor_poly * target_poly + out_variable_poly
)

