from tabulate import tabulate
# This is a more usable version of valuate.sage, less instructional

K.<x, y> = GF(11)[]
Px, Py = K(2), K(4)
S = K.quotient(y^2 - x^3 - 4*x).fraction_field()
X, Y = S(x), S(y)

EC_A = 4
EC_B = 0
EC = y^2 - x^3 - EC_A*x - EC_B

original_f = (y - 2*x)^2
b0, b1, b2 = [(x - Px), (y - Py), 1]

# Return components for basis
def decomp(f):
    a0, r = f.quo_rem(b0)
    a1, r = r.quo_rem(b1)
    a2, r = r.quo_rem(b2)
    assert r == 0
    return [a0, a1, a2]

def comp(comps):
    return sum(a*b for a, b in zip(comps, (b0, b1, b2)))

assert comp(decomp(original_f)) == original_f

# so we can replace (y - Py) with this
Ef = b0^2 + binomial(3,2)*Px*b0^1 + (3*Px^2 + EC_A)
Eg = (y + Py)
assert EC == b1*Eg - b0*Ef

def apply_reduction(a, g):
    assert a[2] == 0
    a[0] = a[0]*Eg + a[1]*Ef
    a[1] = 0
    g[0] *= Eg

k = 0
a = [original_f, 0, 0]
g = [1]

table = []
table.append(("", "a", "g", "k"))

def log(step_name, a, g, k):
    table.append((step_name, str(a), str(g), k))

log("start", a, g, k)

while True:
    f = a[0]
    a = decomp(f)
    log("decomp", a, g, k)

    # Check remainder
    if a[2] != 0:
        break

    # We can apply a reduction
    k += 1

    apply_reduction(a, g)
    log("reduce", a, g, k)

print(tabulate(table))
print(f"k = {k}")
