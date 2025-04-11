K = GF(3)

V = VectorSpace(K, 4)
C = V.subspace([
    [1, 1, 1, 0],
    [0, 1, 2, 1]
])

print(list(C))

c = C.random_element()
assert set(tuple(v - c) for v in C) == set(tuple(v) for v in C)

