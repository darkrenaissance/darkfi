q = 7
n = 6
k = 4
d = n - k + 1

K = GF(q)
F.<z> = K[]

f = 4 + 3*z + 1*z^2 + 5*z^3

# Let βᵢ = K(i - 1)

f_β1 = f(z=0)
f_β2 = f(z=1)
f_β3 = f(z=2)
f_β4 = f(z=3)
f_β5 = f(z=4)
f_β6 = f(z=5)
f_β7 = f(z=6)

f = vector([f_β1, f_β2, f_β3, f_β4, f_β5, f_β6, f_β7])

g0 = vector(a^0 for a in K)
g1 = vector(a^1 for a in K)
g2 = vector(a^2 for a in K)
g3 = vector(a^3 for a in K)
g4 = vector(a^4 for a in K)
g5 = vector(a^5 for a in K)
g6 = vector(a^6 for a in K)

f1 = 4*g0 + 3*g1 + 1*g2 + 5*g3
assert f == f1

