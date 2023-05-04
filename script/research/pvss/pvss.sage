# Publicly Verifiable Secret Sharing
# https://www.win.tue.nl/~berry/papers/crypto99.pdf
# This scheme depends on an honest dealer.

t = 3  # Threshold
n = 5  # Participants

# Pallas
p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
Fp = GF(p)
Fq = GF(q)
Ep = EllipticCurve(Fp, (0, 5))
Ep.set_order(q * 0x01)
Eq = EllipticCurve(Fq, (0, 5))
Eq.set_order(p * 0x01)

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
    #x_i = Fq(i+2)
    x.append(x_i)
    y.append(G * x_i)

# ============
# Distribution
# ============
# The dealer selects a secret s:
s = Fq.random_element()
#s = Fq(42)

# The dealer picks a random polynomial p of degree at most t-1 with
# coefficients in Fp and sets s=alpha_0
alpha = []
for i in range(t):
    alpha.append(Fq.random_element())
    #alpha.append(Fq(i+2))
alpha[0] = s
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
# for 1 ≤ i ≤ n , using the public keys of the participants
Y = []
for i in range(1, n+1):
    Y.append(y[i-1] * p(i))

# Finally, let X_i = prod_{j=0}^{t-1} C_j * i^j. The verifier computes
# this from the published commitments.
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
# TODO: See DLEQ in the paper.

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
    S_i = Y[i] * (1 / x[i])
    assert S_i == G * p(i+1)
    S.append(S_i)

# Pooling the shares. Sample a set of t shares and reconstruct secret.
# FIXME: This is for 1,...,t, we should be able to take random ones.
shares = S[:t]
pooled = Ep(0)

def lambda_func(i, t):
    lambda_i = Fq(1)
    for j in range(1, t+1):
        if j != i:
            lambda_i *= Fq(j) * (Fq(j-i))**(-1)
    return lambda_i

for i in range(t):
    pooled += shares[i] * lambda_func(i+1, t)

# Assert reconstructed secret
assert G*s == G*p(0) == pooled
