# $ sage -sh
# $ pip install tabulate
from tabulate import tabulate
# P = (2, 4)
# ord_P(y - 2x) = 2
# from Washington example 11.4 page 345

K.<x, y> = GF(11)[]
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

original_f = y - 2*x
assert comp(decomp(original_f, basis), basis) == original_f

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

EC_A = 4
EC_B = 0

EC = y^2 - x^3 - EC_A*x - EC_B

# so we can replace (y - Py) with this
b0, b1, b2 = basis
Ef = b0^2 + binomial(3,2)*Px*b0^1 + (3*Px^2 + EC_A)
Eg = (y + Py)
assert EC == b1*Eg - b0*Ef

# f / g
# Technically we don't need g but we keep track of it anyway
def apply_reduction(f, g, basis):
    #a1 = f[1]
    #f[1] = 0

    b0, b1, _ = basis
    # b1 == b0 * f / g
    # so we can replace c b1 with (cf/g) b0

    # a2 = 0
    assert f[2] == 0
    # note that
    #   b1 = (f/g) b0
    # so
    #   x = a0 b0 + a1 b1 + 0 b2
    #     = (a0 + a1 f/g) b0
    # let a0 = p/q
    #   x = (pg + a1 f)
    #       ----------- b0
    #           qg

    f[0] = f[0]*Eg + f[1]*Ef
    f[1] = 0
    g[0] *= Eg

k = 1

table = []
table.append(("", "f", "g", "k"))

def log(step_name, f, g, k):
    table.append((step_name, str(f), str(g), k))

f = decomp(original_f, basis)
g = [1]
log("start", f, g, k)

# Reduce
apply_reduction(f, g, basis)
log("reduce", f, g, k)

f = f[0]
# Decompose
f = decomp(f, basis)
log("decomp", f, g, k)

assert comp(f, basis) == (x - 2)^2 - 5*(x - 2) - 2*(y - 4)
assert f[2] == 0
k += 1

# Reduce
apply_reduction(f, g, basis)
log("reduce", f, g, k)

f = f[0]
# Decompose
f = decomp(f, basis)
log("decomp", f, g, k)

# Program terminates because remainder is nonzero
assert f[2] != 0

print(f"basis = {basis}")
print(tabulate(table))
print(f"k = {k}")

# Test final value is correct
S = K.quotient(y^2 - x^3 - 4*x).fraction_field()
f0, f1, f2 = f
f = f0*b0 + f1*b1 + f2*b2
g = g[0]
fprime = b0^k * f/g
assert fprime == S(original_f)
# to convert fprime back again:
#f, g = fprime.numerator().lift(), fprime.denominator().lift()
assert g(Px, Py) != 0
assert f(Px, Py) != 0
assert b0(Px, Py) == 0

