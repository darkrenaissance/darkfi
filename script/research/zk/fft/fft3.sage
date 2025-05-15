p = 199
n = 4

assert p.is_prime()

def find_ext_order(p, n):
    N = 1
    while True:
        pNx_order = p^N - 1

        # Does n divide the group order 𝔽_{p^N}^×?
        if pNx_order % n == 0:
            return N

        N += 1

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
print(f"p = {p}")
print(f"n = {n}")
print(f"N = {N}")
print(f"p^N = {p^N}")
K.<a> = GF(p^N, repr="int")
ω = find_nth_root_unity(K, n)
print(f"ω = {ω}")
print()

L.<X> = K[]

f = 10*X + 110
f = X^3 + 10*X + 110
#assert f.degree() < n/2
print(f"f = {f}")
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

def dot(a, b):
    assert len(a) == len(b)
    return [a_i*b_i for a_i, b_i in zip(a, b)]

assert n == 2^2
m = 4
print(f"m = {m}")
ω_powers = vector(ω^i for i in range(m/2))
print(f"ω^i = {ω_powers}")

fT = vectorify(f)
print(f"fT  = {fT}")

# Rewrite f(X) = g(X) + X^(n/2) h(X)
f_g, f_h = vector(fT[:m/2]), vector(fT[m/2:])
print(f"    = {f_g}, {f_h}")

r8 =  f_g + f_h
s8 = dot((f_g - f_h), ω_powers)
assert len(r8) == len(s8) == m/2
print(f"r8 = {r8}")
print(f"s8 = {s8}")
print()

m = 2
ω_powers = vector(ω_i for ω_i in ω_powers[::2])
assert len(ω_powers) == m/2
print(f"m = {m}")

# Corresponds to r_4
r4_g, r4_h = vector(r8[:m/2]), vector(r8[m/2:])
print(f"r4_g, r4_h = {r4_g}, {r4_h}")

r4_r2 = r4_g + r4_h
r4_s2 = dot((r4_g - r4_h), ω_powers)
print(f"r4_r2 = {r4_r2}")
print(f"r4_s2 = {r4_s2}")
print()

# Corresponds to s_4
s4_g, s4_h = vector(s8[:m/2]), vector(s8[m/2:])
print(f"s4_g, s4_h = {s4_g}, {s4_h}")

s4_r2 = s4_g + s4_h
s4_s2 = dot((s4_g - s4_h), ω_powers)
print(f"s4_r2 = {s4_r2}")
print(f"s4_s2 = {s4_s2}")
print()

# Final step
m = 1
print(f"m = {m}")
print("STOP")
# Just return the values directly
print()

f_evals = [f(X=ω^i) for i in range(n)]
print(f"f(ω^i) = {f_evals}")

