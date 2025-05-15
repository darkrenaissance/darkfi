# sage: w.multiplicative_order()
# 1330
# sage: 11^3 - 1
# 1330
# sage: K.<w> = GF(11^3, repr="int")
# sage: w
# 11

p = 199
# sage: factor(199^3 - 1)
# 2 * 3^3 * 11 * 13267
n = 3^3
n = 10
n = 5

assert p.is_prime()

def find_ext_order(p, n):
    N = 1
    while True:
        pNx_order = p^N - 1

        # Does n divide the group order 𝔽_{p^N}^×?
        if pNx_order % n == 0:
            return N

        N += 1

# Alternative to the above fn. Technically we still need to loop since
# discrete_log() is a bruteforce algo.
def find_ext_order_alt(p, n):
    # We have that n | p^N - 1 for some n. This is the same as wrtiting:
    #   p^N - 1 = ns for some s
    #   => p^N - 1 ≡ 0 (mod n)
    #   p · p^(N - 1) ≡ 1 (mod n)
    # But recall that p^(N - 1) ≡ p^-1
    # So we just take p (mod n), find its inverse then compute N - 1
    R = Integers(n)
    p = R(p)
    N_minus_1 = discrete_log(p^-1, p)
    return N_minus_1 + 1

def find_nth_root_unity(K, n):
    # It cannot be a quadratic residue if n is odd
    #assert n % 2 == 1

    # So there is an nth root of unity in p^N. Now we have to find it.
    pNx_order = p^N - 1
    ω = K.multiplicative_generator()
    ω = ω^(pNx_order/n)
    assert ω^n == 1
    assert ω^(n - 1) != 1

    return ω

N = find_ext_order(p, n)
print(f"N = {N}")
print()
K.<a> = GF(p^N, repr="int")
ω = find_nth_root_unity(K, n)

L.<X> = K[]

f = X^2 + 2*X + 4
g = 2*X^2 + 110
assert f.degree() < n/2
assert g.degree() < n/2
assert f.degree() + g.degree() < n
print(f"f = {f}")
print(f"g = {g}")
print(f"fg = {f*g}")
print()

def vectorify(f):
    assert f.degree() < n
    fT = vector([f[i] for i in range(f.degree() + 1)] +
                # Zero padding
                [0 for _ in range(n - f.degree() - 1)])
    assert len(fT) == n
    # Just check decomposed polynomial is in the correct order
    assert sum([fT[i]*X^i for i in range(n)]) == f
    return fT

fT = vectorify(f)
gT = vectorify(g)

def nXn_vandermonde(n, ω):
    # We hardcode this one so you know what is looks like
    if n == 5:
        Vω = matrix([
            [1,   1,   1,   1,   1],
            [1, ω^1, ω^2, ω^3, ω^4],
            [1, ω^2, ω^4, ω^1, ω^3],
            [1, ω^3, ω^1, ω^4, ω^2],
            [1, ω^4, ω^3, ω^2, ω^1],
        ])
        return Vω

    # This is the code to generate it
    Vω = matrix([[ω^(i * j) for j in range(n)] for i in range(n)])
    return Vω

Vω = nXn_vandermonde(n, ω)
Vω_inv = nXn_vandermonde(n, ω^-1)/n
# Lemma: V_ω^{-1} = 1/n V_{ω^-1}
assert Vω^-1 == Vω_inv

DFT_ω_f = Vω * fT
f_evals = [f(X=ω^i) for i in range(n)]
print(f"DFT_ω(f) = {DFT_ω_f}")
print(f"f(ω^i) = {f_evals}")
print()

DFT_ω_g = Vω * gT
g_evals = [g(X=ω^i) for i in range(n)]
print(f"DFT_ω(g) = {DFT_ω_g}")
print(f"g(ω^i) = {g_evals}")
print()

def convolution(f, g):
    return f*g % (X^n - 1)
def pointwise_prod(fT, gT):
    return [a_i*b_i for a_i, b_i in zip(fT, gT)]

print(f"deg(f) + deg(g) = {f.degree() + g.degree()}")
fжg = convolution(f, g)
print(f"f☼g = {fжg}")
assert fжg == f*g
fжgT = vectorify(fжg)

DFT_ω_fжg = Vω * fжgT
for i in range(n):
    assert fжg(X=ω^i) == f(ω^i)*g(ω^i)
print(f"DFT_ω(f☼g) = {DFT_ω_fжg}")
DFT_fg_prod = vector(pointwise_prod(DFT_ω_f, DFT_ω_g))
print(f"DFT_ω(f)·DFT_ω(g) = {DFT_fg_prod}")
assert DFT_ω_fжg == DFT_fg_prod

inv_DFT_fg = Vω_inv * DFT_fg_prod
fgT = vectorify(f*g)
assert inv_DFT_fg == fgT
print(f"DFT^-1(DFT_ω(f)·DFT_ω(g)) = {inv_DFT_fg}")

