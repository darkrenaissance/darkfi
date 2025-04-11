K = GF(5)

n = 5
d = 3

# Ambient vector space
V = VectorSpace(K, n)

assert V([1, 1, 0, 0, 0]).hamming_weight() == 2

def is_codespace(W, d):
    return all(v.hamming_weight() >= d for v in W if v != 0)

done = False
for j in range(2, n+1):
    print(f"trying k={j}")
    for W in V.subspaces(j):
        if is_codespace(W, d):
            C = W
            done = True
            break
    if done:
        break

k = C.dimension()

E = V.subspace([
    [1, 0, 0, 0, 0],
    [0, 1, 0, 0, 0],
])

#assert C.dimension() == k
assert E.dimension() == d - 1
assert C.intersection(E).dimension() == 0
assert C.dimension() + E.dimension() == (C + E).dimension()

# But C + E are both subspaces of V which is dim N
assert (C + E).dimension() <= n

