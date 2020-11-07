from bls_py import bls12381
from finite_fields.modp import IntegersModP
from finite_fields.polynomial import polynomialsOver

n = bls12381.n

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
