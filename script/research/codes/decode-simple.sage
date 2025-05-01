import itertools
from tabulate import tabulate

q = 5
n = 4
k = 2
d = n - k + 1

K = GF(q)
R.<x> = K[]
α = K(2)
assert set(α^i for i in range(q - 1)) | {0} == set(K)

# Pick a random codeword
m = (3, 2)
f = m[0] + m[1]*x

c = [f(α^i) for i in range(n)]
assert len(c) == n

# We can tolerate <= (n-k)/2 = 1 error
c[1] = 0

# f is degree 3 so we need 4 points to reconstruct it
table = []
count = 0
total = 0
for i0 in range(n):
    for i1 in range(i0+1, n):
        g = R.lagrange_polynomial([(α^i0, c[i0]), (α^i1, c[i1])])
        table.append([
            (i0, i1),
            g,
            "*" if f == g else None
        ])
        if f == g:
            count += 1
        total += 1
print(tabulate(table))
print(f"{count} / {total}")

