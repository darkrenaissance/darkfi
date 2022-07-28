# $ sage -sh
# $ pip install tabulate
from tabulate import tabulate
# P = (2, 4)
# ord_P(y - 2x) = 2
# from Washington example 11.4 page 345

K.<x, y> = Integers(11)[]
Px, Py = K(2), K(4)

assert (3*Px^2 + 4) / (2*Py) == 2

basis = [(x - Px), (y - Py), 1]
# Return components for basis
def decomp(f, basis):
    comps = []
    r = f
    for b in basis:
        a, r = r.quo_rem(b)
        comps.append(a)
    assert r == 0
    return comps

def comp(comps, basis):
    return sum(a*b for a, b in zip(comps, basis))

f = y - 2*x
assert comp(decomp(f, basis), basis) == f

# P = (a, b)
# y² = x³ + Ax + B
# (y - b)(y + b) = (x - a)³ + C(3,2)a(x - a)² + (3a² + A)(x - a)
#
# sage: ((x - a)^3 + binomial(3,2)*a*(x - a)^2 + (3*a^2 + A)*(x - a)).expand()
# -a^3 + x^3 - A*a + A*x
# But since (a, b) ∈ E(K) => b² = a³ + Aa + B
#                         => B = b² - (a³ + Aa)
#
# So at every step we replace the component for (y - Py)
# with the reduction to the component for (x - Px)

A = 4
B = 0

E = y^2 - x^3 - A*x - B

# f / g
# Technically we don't need g but we keep track of it anyway
def apply_reduction(comp_f, comp_g, basis):
    #a1 = comp_f[1]
    #comp_f[1] = 0

    b0, b1, _ = basis
    # so we can replace (y - Py) with this
    sub_poly_f = b0^2 + binomial(3,2)*Px*b0^1 + (3*Px^2 + A)
    sub_poly_g = (y + Py)
    assert E == b1*sub_poly_g - b0*sub_poly_f
    # b1 == b0 * f / g
    # so we can replace c b1 with (cf/g) b0

    comp_f[0] = comp_f[0]*sub_poly_g + comp_g[2]*sub_poly_f
    comp_g[2] *= sub_poly_g

k = 1

table = []
table.append(("", "f", "g", "k"))

def log(step_name, comp_f, comp_g, k):
    table.append((step_name, str(comp_f), str(comp_g), k))

comp_f = decomp(f, basis)
comp_g = [0, 0, 1]
log("start", comp_f, comp_g, k)

# Reduce
apply_reduction(comp_f, comp_g, basis)
log("reduce", comp_f, comp_g, k)

f = comp_f[0]
# Decompose
comp_f = decomp(f, basis)
comp_g = [0, 0, 1]
log("decomp", comp_f, comp_g, k)

assert comp(comp_f, basis) == (x - 2)^2 - 5*(x - 2) - 2*(y - 4)
assert comp_f[2] == 0
k += 1

# Reduce
apply_reduction(comp_f, comp_g, basis)
log("reduce", comp_f, comp_g, k)

f = comp_f[0]
# Decompose
comp_f = decomp(f, basis)
comp_g = [0, 0, 1]
log("decomp", comp_f, comp_g, k)

# Program terminates because remainder is nonzero
assert comp_f[2] != 0

print(f"basis = {basis}")
print(tabulate(table))
print(f"k = {k}")

