# Scrape PVSS
# https://eprint.iacr.org/2017/216.pdf
from hashlib import sha256

t = 3  # Threshold
n = 5  # Participants
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
