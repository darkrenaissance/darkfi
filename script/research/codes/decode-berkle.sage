import itertools

q = 11
n = 10
k = 4
d = n - k + 1

K = GF(q)
R.<x> = K[]
α = K(2)
assert set(α^i for i in range(q - 1)) | {0} == set(K)

# Pick a random codeword
m = (3, 0, 7, 9)
f = m[0] + m[1]*x + m[2]*x^2 + m[3]*x^3
print(f"m = {m}")

c = vector(f(α^i) for i in range(q - 1))
assert len(c) == q - 1
print(f"c = {c}")

e = int((n - k)/2)
assert e == 3
# We can tolerate <= (n-k)/2 = 3 errors
c[2] = 0
c[4] = 6
c[7] = 7

# Naive and very slow
freqs = {}
for (i0, i1, i2, i3) in itertools.permutations(range(n), int(4)):
    g = R.lagrange_polynomial([
        (α^i0, c[i0]),
        (α^i1, c[i1]),
        (α^i2, c[i2]),
        (α^i3, c[i3]),
    ])
    if not g in freqs:
        freqs[g] = 0
    freqs[g] += 1
max_key = max(freqs.keys(), key=lambda k: freqs[k])
assert max_key == f

E0 = (x - α^2)*(x - α^4)*(x - α^7)
assert E0.degree() <= n - k - 1
N0 = E0*f
print(f"E = {E0}")
print(f"N = {N0}")

S = []
for i in range(n):
    row = []

    αi = α^i

    # deg N = e + (k - 1)
    for j in range(e + k):
        row.append(αi^j)

    r_i = c[i]
    # deg E = e
    # We don't need x^e here
    for j in range(e):
        row.append(-r_i * αi^j)

    assert n == 2*e + k
    assert len(row) == n
    S.append(row)

assert len(S) == n
A = matrix(S)
s = vector(r * α^(i*e) for (i, r) in enumerate(c))
print(f"s = {s}")
v = A.solve_right(s)
print(f"A = {A}")
print(f"v = {v}")

Nv, Ev = v[:e + k], v[e + k:]
print(Nv)
print(Ev)
N = sum(Ni * x^i for (i, Ni) in enumerate(Nv))
E = sum(Ei * x^i for (i, Ei) in enumerate(Ev)) + x^e
print(f"N = {N}")
print(f"E = {E}")
assert N == N0
assert E == E0

f, rem = N.quo_rem(E)
assert rem == 0
print(f"f = {f} =", list(f))

