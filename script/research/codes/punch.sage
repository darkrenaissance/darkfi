from tabulate import tabulate
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
M = V.subspace([
    [1, 0, 0, 0, 0, 0, 0],
    [0, 1, 0, 0, 0, 0, 0],
    [0, 0, 1, 0, 0, 0, 0],
])
table = []
for m in M:
    f = m[0] + m[1]*z + m[2]*z^2

    c = vector(f(β) for β in list(K)[:n])
    assert len(c) == n

    if c.is_zero():
        continue

    table.append((c, c.hamming_weight()))
    assert d <= c.hamming_weight()

print(tabulate(table))
print(d)
