q = 61
K = GF(q)
E = EllipticCurve(K, [8, 1])

P = E(57,24)
Q = E(25,37)
R = E(17,32)
S = E(42,35)
inf = E(0)

D1 = ((1, P), (1, Q), (1, R))
D2 = ((4, inf), (-1, S))

# D1 ~ D2 <=> D1 = D2 + div(f)
# sum(div(f)) = O, deg(div(f)) = 0

def dsum(D):
    P = inf
    for order, pnt in D:
        P += order * pnt
    return P

def degree(D):
    return sum(order for order, _ in D)

def neg(D):
    return tuple((-order, pnt) for order, pnt in D)

D = D1 + neg(D2)
assert degree(D) == 0
assert dsum(D) == inf

