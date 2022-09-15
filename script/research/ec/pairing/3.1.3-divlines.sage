q = 47
Fq = GF(q)
E = EllipticCurve(Fq, (0, 5))

R.<x, y> = PolynomialRing(Fq)

def add(P, Q):
    Px, Py = P[0], P[1]
    Qx, Qy = Q[0], Q[1]
    # We don't handle this case yet :p
    assert Px != Qx
    gradient = (Qy - Py) / (Qx - Px)
    intersect = Py - gradient * Px
    f = y - gradient * x - intersect
    assert f(Px, Py) == 0
    assert f(Qx, Qy) == 0
    R = P + Q
    Rx, Ry = R[0], R[1]
    assert f(Rx, -Ry) == 0
    g = x - Rx
    assert g(Rx, Ry) == g(Rx, -Ry) == 0
    return f / g

P = E(33, 9)
Q = E(34, 39)
# div(f) = [P] + [Q] - [P + Q] - [∞] ∈ Pic(E)
#        = ([P] + [Q] - 2[∞]) - ([P + Q] - [∞])
# => [P] + [Q] - 2[∞] ~ [P + Q] - [∞]
f = add(P, Q)
print(f)

