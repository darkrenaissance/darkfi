# Distributed Key Generation scheme

t = 4   # Threshold
n = 10  # Participants
assert t <= n

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

# The participants create their random polynomials and broadcast shares.
for i in range(n):
    # Pick a random secret
    coeffs = [Fq.random_element()]
    # Generate random polynomial of degree t
    for _ in range(t):
        coeffs.append(Fq.random_element())
    polynomials.append(coeffs)

    # Compute and send secret shares
    shares[i+1] = [sum([coeffs[j] * (k**j) for j in range(t+1)]) for k in range(1, n+1)]
    # Compute public value
    public_values.append(coeffs[0] * G)
    # Broadcast evaluations of the polynomial at public points
    broadcast_values[i+1] = [sum([coeffs[j] * (k**j) for j in range(t+1)]) * G for k in range(1, n+1)]

# ====================
# Step 2: Verification
# ====================

# In real-world, it is important to ensure that malicious participants
# cannot raise false complaints to disqualify honest participants.
# Having a robust mechanism to protect against Sybil attacks or malicious
# complaint flodding is essential.
complaints = {}

# Initial check against broadcasted values
for i in range(1, n+1):
    for j in range(1, n+1):
        if shares[i][j-1] * G != broadcast_values[i][j-1]:
            if j not in complaints:
                complaints[j] = []
            complaints[j].append(i)

# Handle complaints
for complainant, offenders in complaints.items():
    for offender in list(offenders):  # Using list() to avoid runtime modification issues
        # The offender proves they sent a correct share to the complainant
        revealed_share = sum([polynomials[offender-1][k] * complainant**k for k in range(t+1)])
        if revealed_share * G == broadcast_values[offender][complainant-1]:
            complaints[complainant].remove(offender)

# Disqualification step
disqualified = {i for i, comp in complaints.items() if len(comp) > 0}

# ===================================
# Step 3: Secret Share Reconstruction
# ===================================

# Sum the secrets of qualified participants to get the group's secret share
# In a real-world application, this isn't safe and measures should be in
# place to prevent this scenario.
qualified_shares = [polynomials[i][0] for i in range(n) if i+1 not in disqualified]
if len(qualified_shares) < t+1:
    raise ValueError("Too many disqualifications. DKG failed.")

group_secret = sum(qualified_shares)
group_public_0 = group_secret * G

# However we can also do this without ever giving a single party enough
# shares to reconstruct the secret:
participant_pubkeys = [polynomials[i][0] * G for i in range(n) if i+1 not in disqualified]
if len(participant_pubkeys) < t+1:
    raise ValueError("Too many disqualifications. DKG failed.")

group_public_1 = sum(participant_pubkeys)
assert group_public_0 == group_public_1
