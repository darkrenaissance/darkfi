# Publicly Verifiable Secret Sharing
# https://www.win.tue.nl/~berry/papers/crypto99.pdf
# This scheme depends on an honest dealer.
from random import sample
from hashlib import sha256
from itertools import chain

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

# NullifierK Generator: G
nfk_x = 0x25e7aa169ca8198d2e375571faf4c9cf5e7eb192ccb5db9bd36f6aa7e447ca75
nfk_y = 0x155c1f851b1a3384880473442008ff755fe0a49ec1c1b4332db8dce21ae001cc
G = Ep([nfk_x, nfk_y])

# ==============
# Initialization
# ==============
# The participants create their keypairs and register their public keys
# y_i = G^{x_i}
x = []
y = []
for i in range(n):
    x_i = Fq.random_element()
    x.append(x_i)
    y.append(G * x_i)

# ============
# Distribution
# ============
# The dealer selects a secret s:
s = Fq.random_element()

# The dealer picks a random polynomial p of degree at most t-1 with
# coefficients in Fq and sets s=alpha_0
alpha = [s]
for i in range(t-1):
    alpha.append(Fq.random_element())
R.<ω> = PolynomialRing(Fq)
p = R(alpha)
assert p.degree() == t-1
assert p.coefficients()[0] == s

# The dealer keeps this polynomial secret but publishes the related
# commitments C_j = g ^ {a_j} , for 0 ≤ j < t
C = []
for j in range(t):
    C.append(g * alpha[j])

# The dealer also publishes the encrypted shares Y_i = y_i ^ p(i),
# for 1 ≤ i ≤ n , using the public keys of the participants:
Y = []
for i in range(1, n+1):
    Y.append(y[i-1] * p(i))

# Let X_i = prod_{j=0}^{t-1} C_j * i^j. The verifier computes this from the
# published commitments C_j.
X = []
for i in range(1, n+1):
    X_i = Ep(0)
    for j in range(t):
        X_i += C[j] * (i^j)
    X.append(X_i)

# The dealer shows that the encrypted shares are consistent by
# producing a proof of knowledge of the unique p(i), 1 ≤ i ≤ n,
# satisfying: X_i = g^p(i) , Y_i = y_i^p(i)
for i in range(1, n+1):
    assert X[i-1] == g * p(i)
    assert Y[i-1] == y[i-1] * p(i)

# For a non-interactive proof, we can use the Fiat-Shamir technique.
# (See DLEQ in the paper)
# The prover calculates a1_i and a2_i:
w_i = []
p_a1 = []
p_a2 = []
for i in range(n):
    w = Fq.random_element()
    w_i.append(w)
    a1_i = g * w
    a2_i = y[i] * w
    p_a1.append(a1_i)
    p_a2.append(a2_i)

# And then hashes the necessary values in order to produce c:
assert len(X) == len(Y) == len(p_a1) == len(p_a2)
c_hasher = sha256()
for point in chain(X, Y, p_a1, p_a2):
    x_coord, y_coord = point.xy()
    c_hasher.update(str(x_coord).encode())
    c_hasher.update(str(y_coord).encode())

# Prover publishes c
c = Fq(int(c_hasher.hexdigest(), 16))

# And finally, prover calculates and publishes r_i responses:
r = []
for i in range(1, n+1):
    r_i = w_i[i-1] - p(i) * c
    r.append(r_i)

# The verifier calculates a1_i and a2_i:
# a1_i = g^r_i * X_i^c
# a2_i = y_i^r_i * Y_i^c
v_a1 = []
v_a2 = []
for i in range(n):
    a1_i = (g * r[i]) + (X[i] * c)
    a2_i = (y[i] * r[i]) + (Y[i] * c)
    v_a1.append(a1_i)
    v_a2.append(a2_i)

# And then hashes the necessary values in order to produce c:
v_hasher = sha256()
for point in chain(X, Y, v_a1, v_a2):
    x_coord, y_coord = point.xy()
    v_hasher.update(str(x_coord).encode())
    v_hasher.update(str(y_coord).encode())

v_c = Fq(int(v_hasher.hexdigest(), 16))

# And checks that the hash matches the published c
assert v_c == c

# ==============
# Reconstruction
# ==============
# Using its private key x_i, each participant finds the share S_i = G^p(i)
# from Y_i by computing S_i = Y_i^(1/x_i). They publish S_i plus a proof
# that the value S_i is a correct decryption of Y_i. To this end it
# suffices to prove knowledge of an alpha such that y_i = G^alpha and
# Y_i = S_i^alpha, which is accomplished by the non-interactive version
# of the protocol DLEQ(G, y_i, S_i, Y_i).
S = []
for i in range(n):
    S_i = Y[i] * x[i].inverse_of_unit()
    assert S_i == G * p(i+1)
    S.append(S_i)


# DLEQ proofs for reconstruction
dleq_proofs = []
for i in range(n):
    w = Fq.random_element()
    a1 = G * w
    a2 = S[i] * w

    dleq_hasher = sha256()
    for point in [G, y[i], S[i], Y[i], a1, a2]:
        x_coord, y_coord = point.xy()
        dleq_hasher.update(str(x_coord).encode())
        dleq_hasher.update(str(y_coord).encode())

    c = Fq(int(dleq_hasher.hexdigest(), 16))
    r = w - x[i] * c
    dleq_proofs.append((c, r))

# DLEQ verifications for reconstruction
for i in range(n):
    c, r = dleq_proofs[i]
    a1 = G * r + y[i] * c
    a2 = S[i] * r + Y[i] * c

    dleq_hasher = sha256()
    for point in [G, y[i], S[i], Y[i], a1, a2]:
        x_coord, y_coord = point.xy()
        dleq_hasher.update(str(x_coord).encode())
        dleq_hasher.update(str(y_coord).encode())

    v_c = Fq(int(dleq_hasher.hexdigest(), 16))
    assert v_c == c

# Pooling the shares. Sample a set of t shares and reconstruct secret.
sample_indices = sorted(sample(range(n), t))
sampled_shares = [S[i] for i in sample_indices]
pooled = Ep(0)

def lambda_func(i, t, indices):
    lambda_i = Fq(1)
    for j in indices:
        if j != i:
            lambda_i *= Fq(j+1) / (Fq(i+1) - Fq(j+1))
    return lambda_i

for idx, share in zip(sample_indices, sampled_shares):
    pooled += share * lambda_func(idx, t, sample_indices)

# Assert reconstructed secret
assert G*s == G*p(0) == pooled
