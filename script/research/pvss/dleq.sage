# What is defined as DLEQ(g,x,h,y) for proving knowledge of some α
# in zero-knowledge is what follows:
# We want to prove knowledge of a value α ∈ Fq, such that x=g*α and y=h*a,
# given g,x,h,y.

# ================
# Parameters setup
# ================
# Pallas curve
p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
Fp = GF(p)
Fq = GF(q)
Ep = EllipticCurve(Fp, (0, 5))
Ep.set_order(q)

g = Ep.random_point()
h = Ep.random_point()

# Value alpha
α = Fq.random_element()

# =================
# Interactive proof
# =================
# The public data is
x = g*α
y = h*α

# 1. The prover computes a_1 = g*w and a_2 = h*w, where w is a random element
#    of Fq, and sends a_1 and a_2 to the verifier.
w = Fq.random_element()
a_1 = g * w
a_2 = h * w

# 2. The verifier sends a challenge e from Fq to the prover
e = Fq.random_element()

# 3. The prover sends a response z = w - αe to the verifier.
z = w - α * e

# 4. The verifier checks the following and accepts the proof if it holds:
assert a_1 == g*z + x*e
assert a_2 == h*z + y*e

# =====================
# Non-interactive proof
# =====================
# This sigma proof can be transformed into a non-interactive ZK proof
# through the Fiat-Shamir heuristic:
from hashlib import sha256

# Prover:
e = sha256()
e.update(str(x).encode())
e.update(str(y).encode())
e.update(str(a_1).encode())
e.update(str(a_2).encode())
e_prover = Fq(int(e.hexdigest(), 16))
z = w - α * e_prover

# Verifier
e = sha256()
e.update(str(x).encode())
e.update(str(y).encode())
e.update(str(a_1).encode())
e.update(str(a_2).encode())
e_verifier = Fq(int(e.hexdigest(), 16))
assert a_1 == g*z + x*e_verifier
assert a_2 == h*z + y*e_verifier
assert e_prover == e_verifier
