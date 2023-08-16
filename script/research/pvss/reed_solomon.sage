# Reed Solomon check in an elliptic curve context, used in scrape.sage

t = 3   # Threshold
n = 10  # Participants

# Pallas
p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
Fp = GF(p)
Fq = GF(q)
Ep = EllipticCurve(Fp, (0, 5))
Ep.set_order(q)

g = Ep.random_point()
h = Ep.random_point()

# Secret
s = Fq.random_element()

alpha = [s]
for i in range(t-1):
    alpha.append(Fq.random_element())
R.<ω> = PolynomialRing(Fq)
poly = R(alpha)

# Secret shares
shares = [poly(i) for i in range(1, n+1)]
# Share commitments
commits = [g * share for share in shares]

# Reed Solomon check
RS.<σ> = PolynomialRing(Fq)
σ_coeff = [Fq.random_element() for _ in range(n-t)]
v_poly = RS(σ_coeff)
assert v_poly.degree() == n-t-1

v_p = Ep(0)
for i in range(n):
    c_perp = v_poly(Fq(i))
    for j in range(n):
        if i != j:
            c_perp *= (Fq(i)-Fq(j)).inverse()

    v_p += commits[i] * c_perp

assert v_p == Ep(0)
