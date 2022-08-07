F = GF(47)
K.<x, y> = F[]
EC_A, EC_B = 4, 0
E = EllipticCurve(F, [EC_A, EC_B])
C = E.defining_polynomial()
S = K.quotient(C(x, y, 1)).fraction_field()
X, Y = S(x), S(y)

inf = E[0]

# If we have points sharing the same x value then construct a vertical
# line through them to eliminate them.

points = [(34, 30), (44, 14), (7, 18), (28, 31), (27, 45), (12, 15),
          (43, 22), (11, 23), (38, 9), (0, 1),   (26, 33)]

def lagrange_basis(x_k, domain):
    assert x_k in domain
    domain = [x_i for x_i in domain if x_i != x_k]
    assert x_k not in domain

    l = 1
    for x_i in domain:
        l *= (x - x_i)
    l /= l(x_k, 0)

    # Check everything is correct
    assert l(x_k, 0) == 1
    for x_i in domain:
        assert l(x_i, 0) == 0
    return l

print(f"P = {points}")
domain = [Px for Px, _ in points]

f = 0
for Px, Py in points:
    f += Py * lagrange_basis(Px, domain)
# Now make it zero at all the y values
f = y - f
# Check polynomial is correct
for Px, Py in points:
    assert f(Px, Py) == 0

# Now find remaining points in the support
I = ideal([C(x, y, 1), f])
V = [(info[x], info[y]) for info in I.variety()]
print(f"V(I) = {V}")
diff = set(P) - set(V)
print(f"diff = {diff}")

