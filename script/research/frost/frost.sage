# Two-Round Threshold Schnorr Signatures with FROST
# https://datatracker.ietf.org/doc/pdf/draft-irtf-cfrg-frost-14
import os
from hashlib import sha256

# Hacky way to import another sage module:
os.system("sage --preparse frost_util.sage")
os.system("mv frost_util.sage.py frost_util.py")
from frost_util import *

MAX_PARTICIPANTS = 10
MIN_PARTICIPANTS = 4
assert MIN_PARTICIPANTS <= MAX_PARTICIPANTS

# Pallas
p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
Fp = GF(p)
Fq = GF(q)
Ep = EllipticCurve(Fp, (0, 5))
Ep.set_order(q)

# NullifierK Generator: G
nfk_x = 0x25e7aa169ca8198d2e375571faf4c9cf5e7eb192ccb5db9bd36f6aa7e447ca75
nfk_y = 0x155c1f851b1a3384880473442008ff755fe0a49ec1c1b4332db8dce21ae001cc
G = Ep([nfk_x, nfk_y])

# Secret key to share, we assume this is key distribution with trusted dealer.
sk = Fq.random_element()
group_pk = sk * G

alpha = [sk]
for i in range(MIN_PARTICIPANTS-1):
    alpha.append(Fq.random_element())
R.<ω> = PolynomialRing(Fq)
poly = R(alpha)
assert poly.degree() == MIN_PARTICIPANTS-1
assert poly.coefficients()[0] == sk

# Secret key shares
sk_i = [poly(i) for i in range(1, MAX_PARTICIPANTS+1)]

# ======================
# Round One - Commitment
# ======================
# Round one involves each participant generating nonces and their
# corresponding public commitments. A nonce is a pair of Scalar
# values, and a commitment is a pair of elliptic curve points.
# Each participant's behaviour in this round is described by the
# commit function below. Note that this function invokes nonce_generate
# twice, once for each type of nonce produced. The output of this
# function is a pair of secret nonces (hiding_nonce, binding_nonce)
# and their corresponding public commitments (hiding_nonce_commitment,
# binding_nonce_commitment).

# Inputs:
# - secret, a Scalar
# Outputs:
# - nonce, a Scalar
def nonce_generate(secret):
    random_bytes = os.urandom(32)
    return Fq(H3(random_bytes, secret))


# Inputs:
# - sk_i, the secret key share, a Scalar
# Outputs:
# - (nonce, comm), a tuple of nonce and nonce commitment pairs,
#   where each value in the nonce pair is a Scalar and each value
#   in the nonce commitment pair is an elliptic curve point
def commit(sk_i):
    hiding_nonce = nonce_generate(sk_i)
    binding_nonce = nonce_generate(sk_i)
    hiding_nonce_commit = hiding_nonce * G
    binding_nonce_commit = binding_nonce * G

    nonces = (hiding_nonce, binding_nonce)
    commits = (hiding_nonce_commit, binding_nonce_commit)
    return (nonces, commits)


P_nonces = []
P_commits = []
# Either all participants or just the threshold should create nonces.
# It is only important that commit_list has MIN/NUM_PARTICIPANTS.
for i in range(MAX_PARTICIPANTS):
    nonces, commits = commit(sk_i[i])
    P_nonces.append(nonces)
    P_commits.append(commits)

commit_list = []
for (i, (hnc, bnc)) in enumerate(P_commits[:MIN_PARTICIPANTS]):
    commit_list.append((Fq(i+1), hnc, bnc))

# The outputs nonce and comm from participant P_i should both be stored
# locally and kept for use in the second round. The nonce value is secret
# and MUST NOT be shared, whereas the public output comm is sent to the
# Coordinator. The nonce values produced by this function MUST NOT be used
# in more than one invocation of `sign()`, and the nonces MUST be generated
# from a source of secure randomness.

# ======================================
# Round Two - Signature Share Generation
# ======================================
# In round two, the Coordinator is responsible for sending the message to
# be signed, and for choosing which participants will participate (of number
# at least MIN_PARTICIPANTS). Signers additionally require locally held data;
# specifically, their secret key and the nonces corresponding to their
# commitment issued in round one.
# The Coordinator begins by sending each participant the message to be
# signed along with the set of signing commitments for all participants
# in the participant list.

# Inputs:
# - group_pk, the public key corresponding to the group signing key
# - commit_list = [(i, hiding_nonce_commit_i, binding_nonce_commit_i), ...],
#   a list of commitments issued by each participant, where each element
#   indicates a nonzero Scalar identifier i and two commitments which are
#   elliptic curve points. This list MUST be sorted in ascending order by
#   identifier.
# - msg, the message to be signed.
# Outputs:
# - binding_factor_list, a list of (nonzero Scalar, Scalar) tuples
#   representing the binding factors
def compute_binding_factors(group_pk, commit_list, msg):
    msg_hash = Fq(H4(msg))
    encoded_commitment_hash = Fq(H5(encode_group_commitment_list(commit_list)))
    rho_input_prefix = b"".join([
        point_to_bytes(group_pk),
        scalar_to_bytes(msg_hash),
        scalar_to_bytes(encoded_commitment_hash),
    ])

    binding_factor_list = []
    for (ident, hiding_nonce_commit, binding_nonce_commit) in commit_list:
        rho_input = b"".join([rho_input_prefix, scalar_to_bytes(ident)])
        binding_factor = Fq(H1(rho_input))
        binding_factor_list.append((ident, binding_factor))

    return binding_factor_list


def binding_factor_for_participant(binding_factor_list, ident):
    for (i, binding_factor) in binding_factor_list:
        if ident == i:
            return binding_factor
    raise "invalid participant"


def compute_group_commitment(commit_list, binding_factor_list):
    group_commitment = Ep(0)
    for (ident, hiding_nonce_commit, binding_nonce_commit) in commit_list:
        binding_factor = binding_factor_for_participant(binding_factor_list, ident)
        binding_nonce = binding_nonce_commit * binding_factor
        group_commitment += hiding_nonce_commit + binding_nonce

    return group_commitment


def participants_from_commitment_list(commit_list):
    identifiers = []
    for (identifier, _, _) in commit_list:
        identifiers.append(identifier)
    return identifiers


def derive_interpolating_value(L, x_i):
    if x_i not in L:
        raise "invalid parameters"

    for x_j in L:
        if L.count(x_j) > 1:
            raise "invalid parameters"

    numerator = Fq(1)
    denominator = Fq(1)
    for x_j in L:
        if x_j == x_i:
            continue
        numerator *= x_j
        denominator *= x_j - x_i

    value = numerator / denominator
    return value


def compute_challenge(group_commitment, group_pk, msg):
    challenge_input = b"".join([
        point_to_bytes(group_commitment),
        point_to_bytes(group_pk),
        msg,
    ])

    challenge = Fq(H2(challenge_input))
    return challenge


def sign(ident, sk_i, group_pk, nonce_i, msg, commit_list):
    # Compute the binding factor(s)
    binding_factor_list = compute_binding_factors(group_pk, commit_list, msg)
    binding_factor = binding_factor_for_participant(binding_factor_list, ident)

    # Compute the group commitment
    group_commit = compute_group_commitment(commit_list, binding_factor_list)

    # Compute the interpolating value
    participant_list = participants_from_commitment_list(commit_list)
    lambda_i = derive_interpolating_value(participant_list, ident)

    # Compute the per-message challenge
    challenge = compute_challenge(group_commit, group_pk, msg)

    # Compute the signature share
    (hiding_nonce, binding_nonce) = nonce_i
    sig_share = hiding_nonce + (binding_nonce * binding_factor) + \
        (lambda_i * sk_i * challenge)

    return sig_share


# For demo purposes, we'll just pick the first participants in order.
msg = b"Hello FROST"
sig_shares = []
for i in range(MIN_PARTICIPANTS):
    sig_share = sign(Fq(i+1), sk_i[i], group_pk, P_nonces[i], msg, commit_list)
    sig_shares.append(sig_share)


# ===========================
# Signature Share Aggregation
# ===========================
def verify_signature_share(ident, PK_i, comm_i, sig_share_i, commit_list,
                           group_pk, msg):
    binding_factor_list = compute_binding_factors(group_pk, commit_list, msg)
    binding_factor = binding_factor_for_participant(binding_factor_list, ident)

    group_commit = compute_group_commitment(commit_list, binding_factor_list)

    (hiding_nonce_comm, binding_nonce_comm) = comm_i
    comm_share = hiding_nonce_comm + binding_nonce_comm * binding_factor

    challenge = compute_challenge(group_commit, group_pk, msg)

    participant_list = participants_from_commitment_list(commit_list)
    lambda_i = derive_interpolating_value(participant_list, ident)

    l = sig_share_i * G
    r = comm_share + PK_i * (challenge * lambda_i)

    return l == r

# Verify individual signature shares:
for i in range(MIN_PARTICIPANTS):
    assert verify_signature_share(Fq(i+1), sk_i[i] * G, P_commits[i],
                                  sig_shares[i], commit_list, group_pk, msg)


def aggregate(commit_list, msg, group_pk, sig_shares):
    binding_factor_list = compute_binding_factors(group_pk, commit_list, msg)
    group_commit = compute_group_commitment(commit_list, binding_factor_list)

    # Compute aggregated signature
    z = Fq(0)
    for z_i in sig_shares:
        z += z_i
    return (group_commit, z)

group_commit, z = aggregate(commit_list, msg, group_pk, sig_shares)

# ============
# Verification
# ============
c = compute_challenge(group_commit, group_pk, msg)
assert G * z == group_commit + group_pk * c 
