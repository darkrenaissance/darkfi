# MuSig2: Simple Two-Round Schnorr Multi-Signatures
# https://eprint.iacr.org/2020/1261.pdf
# This scheme is n-of-n, not threshold.
from hashlib import sha256

# Nonces
v = 3
# Participants
n = 5

# Pallas
p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
Fp = GF(p)
Fq = GF(q)
Ep = EllipticCurve(Fp, (0, 5))
Ep.set_order(q)

# NullifierK Generator: g
nfk_x = 0x25e7aa169ca8198d2e375571faf4c9cf5e7eb192ccb5db9bd36f6aa7e447ca75
nfk_y = 0x155c1f851b1a3384880473442008ff755fe0a49ec1c1b4332db8dce21ae001cc
g = Ep([nfk_x, nfk_y])

def hash_domain(domain, *args):
    concat = domain.encode() + b"".join(str(arg).encode() for arg in args)
    return Fq(int(sha256(concat).hexdigest(), 16))

# Domain separator for H_agg
def H_AGG(*args):
    return hash_domain("musig2_H_agg", *args)

# Domain separator for H_non
def H_NON(*args):
    return hash_domain("musig2_H_non", *args)

# Domain separator for H_sig
def H_SIG(*args):
    return hash_domain("musig2_H_sig", *args)

# =================
# 1. Key generation
# =================
x = [Fq.random_element() for _ in range(n)]
X = [x_i * g for x_i in x]

# ==================
# 2. Key aggregation
# ==================
L = b"".join(str(X_i).encode() for X_i in X)
X_tilde = Fq(0) * g
for i in range(n):
    a_i = H_AGG(L, X[i])
    X_tilde += X[i] * a_i

# ======================
# 3. First signing round
# ======================
R_i = []  # Each participant's public nonces
r_i = []  # Each participant's secret nonces

for _ in range(n):
    r_j = [Fq.random_element() for _ in range(v)]
    R_j = [r * g for r in r_j]
    r_i.append(r_j)
    R_i.append(R_j)

# Sum up the nonces for all participants for each j
R = [sum(R_ij[j] for R_ij in R_i) for j in range(v)]
assert len(R) == v

# =======================
# 4. Second signing round
# =======================
message = "Hello MuSig2"
s_i = []
b = H_NON(X_tilde, *R, message)
R_total = sum(R[j] * b * (j+1) for j in range(v))

c = H_SIG(X_tilde, R_total, message)  # Compute the challenge based on R_total
for i in range(n):
    a_i = H_AGG(L, X[i])
    s_partial = c * a_i * x[i] + sum(r_i[i][j] * (b * (j+1)) for j in range(v))
    s_i.append(s_partial)

s = sum(s_i)

# ===============
# 5. Verification
# ===============
c = H_SIG(X_tilde, R_total, message)
assert g * s == R_total + X_tilde * c
