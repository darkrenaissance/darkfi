q = 11
k = 3
d0 = q - k + 1

s = 4
assert s <= d0 - 1
n = q - s
d = n - k + 1

K = GF(q)
F.<z> = K[]

V = VectorSpace(K, n)
C = V.subspace([
    [1, 0, 0, 0, 0, 0, 0],
    [0, 1, 0, 0, 0, 0, 0],
    [0, 0, 1, 0, 0, 0, 0],
])
for c in C:
    f = c[0] + c[1]*z + c[2]*z^2

    w = vector(f(β) for β in list(K)[:n])
    assert len(w) == n

    if w.is_zero():
        continue

    assert d <= w.hamming_weight()

