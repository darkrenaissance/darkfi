# Distributed Key Generation
# A naive approach.

t = 4  # Threshold
n = 10  # Participants

# Pallas
p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
Fp = GF(p)
Fq = GF(q)
Ep = EllipticCurve(Fp, (0, 5))
Ep.set_order(q)

# NullifierK Generator: G
G = Ep([0x25e7aa169ca8198d2e375571faf4c9cf5e7eb192ccb5db9bd36f6aa7e447ca75,
        0x155c1f851b1a3384880473442008ff755fe0a49ec1c1b4332db8dce21ae001cc])

# =================================
# Step 1: Secret Share Distribution
# =================================
polynomials = []
shares = {}
public_values = []
broadcast_values = {}

for i in range(n):
    # Generate random polynomial of degree t
    coeffs = [Fq.random_element() for _ in range(t+1)]
    # Set a_0 as a random secret
    coeffs[0] = Fq.random_element()
    polynomials.append(coeffs)

    # Compute and send secret shares
    shares[i+1] = [sum([coeffs[j] * (i+1)**j for j in range(t+1)]) for i in range(n)]
    # Compute public value
    public_values.append(coeffs[0] * G)
    # Broadcast f_i(1)*G, f_i(2)*G, ..., f_i(n)*G
    broadcast_values[i+1] = [sum([coeffs[j] * k**j for j in range(t+1)]) * G for k in range(1, n+1)]

# ====================
# Step 2: Verification
# ====================
complaints = {}
for j in range(1, n+1):  # For each participant P_j
    complaints[j] = []
    for i in range(1, n+1):  # From each participant P_i
        if shares[i][j-1] * G != broadcast_values[i][j-1]:
            complaints[j].append(i)  # P_j complains against P_i

# ===================================
# Step 3: Secret Share Reconstruction
# ===================================
disqualified = {i for i, comp in complaints.items() if len(comp) > t}

# Sum the shares of qualified participants to get the group's secret share
# This is assuming a single party holds enough shares.
qualified_shares = [shares[i][i-1] for i in range(1, n+1) if i not in disqualified]
if len(qualified_shares) < t+1:
    raise Exception("Too many disqualifications. DKG failed.")

group_secret = sum(qualified_shares)
group_public_0 = group_secret * G

# However we can also do this without ever giving a single party enough
# shares to reconstruct the secret:
participant_pubkeys = [shares[i][i-1] * G for i in range(1, n+1) if i not in disqualified]
if len(participant_pubkeys) < t+1:
    raise Exception("Too many disqualifications. DKG failed.")

group_public_1 = sum(participant_pubkeys)
assert group_public_0 == group_public_1
