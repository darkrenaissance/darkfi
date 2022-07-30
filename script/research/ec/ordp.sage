from tabulate import tabulate
# This is a more usable version of valuate.sage, less instructional

# Return components for basis
def decomp(f, basis):
    b0, b1, b2 = basis
    a0, r = f.quo_rem(b0)
    a1, r = r.quo_rem(b1)
    a2, r = r.quo_rem(b2)
    assert r == 0
    return [a0, a1, a2]

def comp(comps, basis):
    return sum(a*b for a, b in zip(comps, basis))

def apply_reduction(a, g, Ef, Eg):
    assert a[2] == 0
    a[0] = a[0]*Eg + a[1]*Ef
    a[1] = 0
    g[0] *= Eg

# EC_A, EC_B must be defined before calling this function
def ordp(P, original_f, debug=False):
    EC = y^2 - x^3 - EC_A*x - EC_B

    Px, Py = P

    if Py != 0:
        b0, b1, b2 = [(x - Px), (y - Py), 1]

        # so we can replace (y - Py) with this
        Ef = b0^2 + binomial(3,2)*Px*b0^1 + (3*Px^2 + EC_A)
        Eg = (y + Py)
        assert EC == b1*Eg - b0*Ef
    else:
        b0, b1, b2 = [y, x, 1]

        # we can replace x with this
        Ef = b0
        Eg = x^2 + EC_A + EC_B

    basis = [b0, b1, b2]

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
        a = decomp(f, basis)
        log("decomp", a, g, k)

        # Check remainder
        if a[2] != 0:
            break

        # We can apply a reduction
        k += 1

        apply_reduction(a, g, Ef, Eg)
        log("reduce", a, g, k)

    if debug:
        print(tabulate(table))

    u = b0
    f = comp(a, basis)
    g = g[0]
    assert u(Px, Py) == 0
    assert f(Px, Py) != 0
    #assert g(Px, Py) != 0

    return k

#K.<x, y> = GF(11)[]
#EC_A = 4
#EC_B = 0
#P = (2, 4)
#f = y - 2*x
#k = ordp(P, f, debug=True)
#print(f"k = {k}")

