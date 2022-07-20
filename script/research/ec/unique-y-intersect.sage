# Lets show that div(f) = [P] - [∞]
# So any divisor with supp(D) = {P},
# with an effective size of 1 can be represented
# by the horizontal line f = y - P.y
q = 47
K = GF(q)
E = EllipticCurve(K, (0, 5))
C = E.defining_polynomial()

R.<x, y> = PolynomialRing(K)

for i in range(100):
    P = E.random_point()
    Px, Py = P[0], P[1]

    # Skip points at infinity
    if P[2] == 0:
        continue
    assert P[2] == 1

    f = y - Py

    I = Ideal([C(x, y, 1), f])
    V = I.variety()
    print(P, V)
    assert len(V) == 1
    assert V[0][x] == Px
    assert V[0][y] == Py

