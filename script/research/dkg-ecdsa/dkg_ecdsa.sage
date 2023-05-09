# https://eprint.iacr.org/2021/060.pdf
from hashlib import sha256

t = 3  # Threshold
n = 5  # Participants

# secp256k1
p = 2^256-2^32-977
q = 0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141
Fp = GF(p)
Fq = GF(q)
Ep = EllipticCurve(Fp, [0, 7])
Ep.set_order(q)

# Base point
g = Ep([
    0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798,
    0x483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8,
])

# Key Generation
# ==============
xi = []
Xi = []
Xi_proofs = []
for i in range(n):
    x_i = Fq.random_element()
    X_i = g * x_i
    xi.append(x_i)
    Xi.append(X_i)

    # Schnorr proof of knowledge
    k = Fq.random_element()
    R = k * g
    hasher = sha256()
    hasher.update(str(X_i.xy()))
    hasher.update(str(R.xy()))
    e = Fq(int(hasher.hexdigest(), 16))
    s = k + e * x_i
    Xi_proofs.append((R, s))
    # Verifier computes: e = hash(X || r),  s*g == R+e*X
