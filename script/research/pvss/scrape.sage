# Scrape PVSS
# https://eprint.iacr.org/2017/216.pdf
from random import sample
from hashlib import sha256

t = 3   # Threshold
n = 10  # Participants
assert t <= n

# Pallas
p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
Fp = GF(p)
Fq = GF(q)
Ep = EllipticCurve(Fp, (0, 5))
Ep.set_order(q)

# ValueCommitR Generator: g
vcr_x = 0x07f444550fa409bb4f66235bea8d2048406ed745ee90802f0ec3c668883c5a91
vcr_y = 0x24136777af26628c21562cc9e46fb7c2279229f1f39281460e2f46c8a772d9ca
g = Ep([vcr_x, vcr_y])

# NullifierK Generator: h
nfk_x = 0x25e7aa169ca8198d2e375571faf4c9cf5e7eb192ccb5db9bd36f6aa7e447ca75
nfk_y = 0x155c1f851b1a3384880473442008ff755fe0a49ec1c1b4332db8dce21ae001cc
h = Ep([nfk_x, nfk_y])

# ==============
# Initialization
# ==============
# Every party P_i publishes a public key pk_i and witholds the corresponding
# secret key sk_i.
sk = []
pk = []
for i in range(n):
    sk_i = Fq.random_element()
    pk_i = h * sk_i
    sk.append(sk_i)
    pk.append(pk_i)

# ============
# Distribution
# ============
# The dealer selects a random secret s:
s = Fq.random_element()

# Pick a polynomial for sharing the secret
alpha = [s]
for i in range(t-1):
    alpha.append(Fq.random_element())
R.<ω> = PolynomialRing(Fq)
poly = R(alpha)
assert poly.degree() == t-1
assert poly.coefficients()[0] == s

# Encrypt the shares
shares = []
for i in range(1, n+1):
    shares.append(poly(i))

enc_shares = []
for (i, share) in enumerate(shares):
    enc_shares.append(pk[i] * share)

# Commit to shares
v = []
for share in shares:
    v.append(g * share)

# Create DLEQ proofs:
# (Remember DLEQ(g,x,h,y) where g and h are generators, x=g*α, y=h*α)
# We calculate DLEQ(g, v_i, pk_i, enc_shares_i):
# x_i = g * share_i = v_i
# y_i = pk_i * share_i = enc_shares_i

w = Fq.random_element()

# Fiat-Shamir:
e = sha256()
e.update(str(g*w).encode())
for i in range(n):
    e.update(str(v[i]).encode())
    e.update(str(enc_shares[i]).encode())
    e.update(str(pk[i] * w).encode())
e_prover = Fq(int(e.hexdigest(), 16))

a1 = g * w
a2 = []
for i in range(n):
    a2.append(pk[i] * w)

z = []
for i in range(n):
    z_i = w - shares[i] * e_prover
    z.append(z_i)

# ============
# Verification
# ============
# Check the DLEQ proof:
e = sha256()
e.update(str(a1).encode())
for i in range(n):
    e.update(str(v[i]).encode())
    e.update(str(enc_shares[i]).encode())
    e.update(str(a2[i]).encode())
e_verifier = Fq(int(e.hexdigest(), 16))

assert e_prover == e_verifier
for i in range(n):
    assert a1 == g*z[i] + v[i]*e_verifier
    assert a2[i] == pk[i]*z[i] + enc_shares[i]*e_verifier

# Reed Solomon check:
# We can sample a polynomial with random coefficients of degree n-t-1
RS.<σ> = PolynomialRing(Fq)
σ_coeff = [Fq.random_element() for _ in range(n-t)]
v_poly = RS(σ_coeff)
assert v_poly.degree() == n-t-1

# Then perform the following:
v_p = Ep(0)
for i in range(n):
    c_perp = v_poly(Fq(i))
    for j in range(n):
        if i != j:
            c_perp *= (Fq(i) - Fq(j)).inverse()
    v_p += v[i] * c_perp

assert v_p == Ep(0)

# At this point we accept the proof and shares as valid.

# ==============
# Reconstruction
# ==============
# Parties decrypt their shares, ~s_i = ^s_i * 1/sk_i = h * s_i
dec_shares = []
for i in range(n):
    share = enc_shares[i] * sk[i].inverse()
    dec_shares.append(share)

# See pvss.sage for DLEQ reconstruction proofs.
# The proof is: DLEQ(h, pk_i, dec_shares_i, enc_shares_i), showing that
# the decrypted share dec_share_i corresponds to enc_shares_i.

def lambda_func(i, t, indices):
    lambda_i = Fq(1)
    for j in indices:
        if j != i:
            lambda_i *= Fq(j+1) / (Fq(i+1) - Fq(j+1))
    return lambda_i

# Pooling the shares. Sample a set of t shares and reconstruct secret.
sample_indices = sorted(sample(range(n), t))
sampled_shares = [dec_shares[i] for i in sample_indices]
pooled = Ep(0)

for idx, share in zip(sample_indices, sampled_shares):
    pooled += share * lambda_func(idx, t, sample_indices)

assert h*s == h*poly(0) == pooled
