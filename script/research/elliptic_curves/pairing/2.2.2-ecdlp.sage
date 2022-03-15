q = 1021
K = GF(q)
E = EllipticCurve(K, [905, 100])
print(E)
print(f"Group order is: {E.cardinality()}")
P = E(1006, 416)
assert P.additive_order() == E.cardinality()

Q = E(612, 827)

matches = {}

for j, m in factor(E.cardinality()):
    assert m == 1

    P_j = int(E.cardinality() / j) * P
    Q_j = int(E.cardinality() / j) * Q

    for k in range(j):
        if k * P_j == Q_j:
            #print(f"Match found for j = {j}!")
            matches[j] = k
            break

k = crt(list(matches.values()), list(matches.keys()))
print(f"k = {k} mod {E.cardinality()}")
