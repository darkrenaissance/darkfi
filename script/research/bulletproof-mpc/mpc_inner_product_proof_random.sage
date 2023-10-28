load('../mpc/curve.sage')
load('proof.sage')
load('transcript.sage')
load('../mpc/beaver.sage')
load('proof_mpc.sage')
import gc
import numpy as np

##
n = 2

#Q = to_ec_shares_list([CurvePoint.generator() for _ in range(0, int(n/2))])
Q = to_ec_shares_list([CurvePoint.generator()])
Q1 = to_ec_shares_list([CurvePoint.random() for _ in range(len(Q))])
Q2 = [q - q1 for q, q1 in zip(Q, Q1)]

H = to_ec_shares_list([CurvePoint.generator() for i in range(0,n)])
H1 = to_ec_shares_list([CurvePoint.random() for i in range(0,n)])
H2 = [h - h1 for h, h1 in zip(H, H1)]

G = to_ec_shares_list([CurvePoint.generator() for i in range(0,n)])
G1 = to_ec_shares_list([CurvePoint.random() for i in range(0,n)])
G2 = [g - g1 for g, g1 in zip(G, G1)]

## source
source = Source(p)
## alpha
party0_val = [random.randint(0,p) for _ in range(0,n)]
party1_val = [random.randint(0,p) for _ in range(0,n)]
party0_random = [random.randint(0,p) for _ in range(0,n)]
alpha1 = [AuthenticatedShare(party0_random[i], source, 0) for i in range(0,n)]
alpha2 = [AuthenticatedShare(party0_val[i] - party0_random[i], source, 0) for i in range(0,n)]
#print('alpha2: {}'.format(alpha2))
a_shares = [alpha1, alpha2]
#print('a shares: {}'.format(a_shares))
## beta
party1_random = [random.randint(0,p) for _ in range(0,n)]
beta1 = [AuthenticatedShare(party1_random[i], source, 1) for i in range(0,n)]
beta2 = [AuthenticatedShare(party1_val[i] - party1_random[i], source, 1) for i in range(0,n)]
b_shares = [beta1, beta2]
#print('b shares: {}'.format(b_shares))
## c
my_c_shares = [MultiplicationAuthenticatedShares(a_share, b_share, source.triplet(0), 0) for a_share, b_share in zip(a_shares[0], b_shares[0])]
their_c_shares = [MultiplicationAuthenticatedShares(peer_a_share, peer_b_share, source.triplet(0), 1) for peer_a_share, peer_b_share in zip(a_shares[1], b_shares[1])]
##
party_0_c_shares = [my_c_share.mul(their_c_share.d.copy(), their_c_share.e.copy()) for my_c_share, their_c_share in zip(my_c_shares, their_c_shares)]
party_1_c_shares = [their_c_share.mul(my_c_share.d.copy(), my_c_share.e.copy()) for my_c_share, their_c_share in zip(my_c_shares, their_c_shares)]

party_0_c_share = [sum_shares(party_0_c_shares, source, 0)]
party_1_c_share = [sum_shares(party_1_c_shares, source, 1)]

##
y_inv = K(1)
##
G_factors = [K(1)]*n
H_factors = [y_inv**i for i in range(0,n)]
##
party_0_b_prime_shares = [b_share.mul_scalar(y) for b_share, y in  zip(b_shares[0], H_factors)]
party_0_a_prime_shares = a_shares[0].copy()
##
party_1_b_prime_shares = [b_share.mul_scalar(y) for b_share, y in  zip(b_shares[1], H_factors)]
party_1_a_prime_shares = a_shares[1].copy()
##
party_0_g_a_prime_shares = MSM(G1, party_0_a_prime_shares, source, 0)
party_0_h_b_prime_shares = MSM(H1, party_0_b_prime_shares, source, 0)
party_0_q_c_shares = MSM(Q1, party_0_c_share, source, 0)
##
party_1_g_a_prime_shares = MSM(G2, party_1_a_prime_shares, source, 1)
party_1_h_b_prime_shares = MSM(H2, party_1_b_prime_shares, source, 1)
party_1_q_c_shares = MSM(Q2, party_1_c_share, source, 1)
## msm multiplication shares announcement for g_a_prime_shares
party_0_g_a_prime_shares_de = [[party_0_g_a_prime_share.d, party_0_g_a_prime_share.e] for party_0_g_a_prime_share in party_0_g_a_prime_shares.point_scalars]
party_1_g_a_prime_shares_de = [[party_1_g_a_prime_share.d, party_1_g_a_prime_share.e] for party_1_g_a_prime_share in party_1_g_a_prime_shares.point_scalars]
party_0_g_a_prime_shares_lhs = party_0_g_a_prime_shares.msm(party_1_g_a_prime_shares_de)
party_1_g_a_prime_shares_rhs = party_1_g_a_prime_shares.msm(party_0_g_a_prime_shares_de)
g_a_prime = party_0_g_a_prime_shares_lhs.authenticated_open(party_1_g_a_prime_shares_rhs)
## msm multiplication shares announcement for g_b_prime_shares
party_0_h_b_prime_shares_de = [[party_0_h_b_prime_share.d, party_0_h_b_prime_share.e] for party_0_h_b_prime_share in party_0_h_b_prime_shares.point_scalars]
party_1_h_b_prime_shares_de = [[party_1_h_b_prime_share.d, party_1_h_b_prime_share.e] for party_1_h_b_prime_share in party_1_h_b_prime_shares.point_scalars]
party_0_h_b_prime_shares_lhs = party_0_h_b_prime_shares.msm(party_1_h_b_prime_shares_de)
party_1_h_b_prime_shares_rhs = party_1_h_b_prime_shares.msm(party_0_h_b_prime_shares_de)
h_b_prime = party_0_h_b_prime_shares_lhs.authenticated_open(party_1_h_b_prime_shares_rhs)
## msm multiplication shares announcement for q_c_prime_shares
party_0_q_c_shares_de = [[party_0_q_c_share.d, party_0_q_c_share.e] for party_0_q_c_share in party_0_q_c_shares.point_scalars]
party_1_q_c_shares_de = [[party_1_q_c_share.d, party_1_q_c_share.e] for party_1_q_c_share in party_1_q_c_shares.point_scalars]
party_0_q_c_shares_lhs = party_0_q_c_shares.msm(party_1_q_c_shares_de)
party_1_q_c_shares_rhs = party_1_q_c_shares.msm(party_0_q_c_shares_de)
q_c = party_0_q_c_shares_lhs.authenticated_open(party_1_q_c_shares_rhs)
## party 0 proof generation
party_0_transcript = Transcript('bulletproof')
party_0_proof = MpcProof(party_0_transcript, Q1, G_factors, H_factors, G1, H1, a_shares[0], b_shares[0], source, 0)
## party 1 proof generation
party_1_transcript = Transcript('bulletproof')
party_1_proof = MpcProof(party_1_transcript, Q2, G_factors, H_factors, G2, H2, a_shares[1], b_shares[1], source, 1)
## create proof L, R
party_1_proof_c_l = party_1_proof.c_l.copy()
party_1_proof_c_r = party_1_proof.c_r.copy()
party_0_proof_c_l = party_0_proof.c_l.copy()
party_0_proof_c_r = party_0_proof.c_r.copy()

c_l_lhs = [party_0_proof_c_l_i[0].mul(party_1_proof_c_l_i[0].d, party_1_proof_c_l_i[0].e) for party_0_proof_c_l_i, party_1_proof_c_l_i in zip(party_0_proof_c_l, party_1_proof_c_l)]
c_l_rhs = [party_1_proof_c_l_i[0].mul(party_0_proof_c_l_i[0].d, party_0_proof_c_l_i[0].e) for party_1_proof_c_l_i, party_0_proof_c_l_i in zip(party_1_proof_c_l, party_0_proof_c_l)]
c_l_res = [c_l_lhs_i.authenticated_open(c_l_rhs_i) for c_l_lhs_i, c_l_rhs_i in zip(c_l_lhs, c_l_rhs)]
#print('c_l: {}'.format(c_l_res))
c_r_lhs = [party_0_proof_c_r_i[0].mul(party_1_proof_c_r_i[0].d, party_1_proof_c_r_i[0].e) for party_0_proof_c_r_i, party_1_proof_c_r_i in zip(party_0_proof_c_r, party_1_proof_c_r)]
c_r_rhs = [party_1_proof_c_r_i[0].mul(party_0_proof_c_r_i[0].d, party_0_proof_c_r_i[0].e) for party_1_proof_c_r_i, party_0_proof_c_r_i in zip(party_1_proof_c_r, party_0_proof_c_r)]
c_r_res = [c_r_lhs_i.authenticated_open(c_r_rhs_i) for c_r_lhs_i, c_r_rhs_i in zip(c_r_lhs, c_r_rhs)]
#print('c_r: {}'.format(c_r_res))

party_0_proof.create(party_1_proof_c_l, party_1_proof_c_r)
party_1_proof.create(party_0_proof_c_l, party_0_proof_c_r)


## expected P
expected_P =  sum([g_a_prime, h_b_prime, q_c])
## party 0 proof verification
party_0_verifier = Transcript('bulletproof')
party_0_proof.calculate_c_shares(n, party_0_verifier, G_factors, H_factors)
## party 1 proof verification
party_1_verifier = Transcript('bulletproof')
party_1_proof.calculate_c_shares(n, party_1_verifier, G_factors, H_factors)
##
party_0_c_shares_de = [[my_c_share.d.copy(), my_c_share.e.copy()] for my_c_share in party_0_proof.my_c_shares]
party_1_c_shares_de = [[my_c_share.d.copy(), my_c_share.e.copy()] for my_c_share in party_1_proof.my_c_shares]
##
party_1_proof_lhs = [[ii.copy() for ii in i] for i in party_1_proof.lhs]
party_0_proof_lhs = [[ii.copy() for ii in i] for i in party_0_proof.lhs]
party_1_proof_rhs = [[ii.copy() for ii in i] for i in party_1_proof.rhs]
party_0_proof_rhs = [[ii.copy() for ii in i] for i in party_0_proof.rhs]
# verify party 1 lpr
party_0_proof.open_lr(Q1, G1, H1, party_1_c_shares_de ,party_1_proof_lhs, party_1_proof_rhs)
# verify party 0 lpr
party_1_proof.open_lr(Q2, G2, H2, party_0_c_shares_de, party_0_proof_lhs, party_0_proof_rhs)
party_0_proof_l = party_0_proof.L
party_1_proof_l = party_1_proof.L
L = sum(party_0_proof_l_i.authenticated_open(party_1_proof_l_i) for party_0_proof_l_i, party_1_proof_l_i in zip(party_0_proof_l, party_1_proof_l))
party_0_proof_r = party_0_proof.R
party_1_proof_r = party_1_proof.R
R = sum(party_0_proof_r_i.authenticated_open(party_1_proof_r_i) for party_0_proof_r_i, party_1_proof_r_i in zip(party_0_proof_r, party_1_proof_r))

# validate proofs
party_0_proof.open_and_validate_P(party_1_proof.res_p, expected_P)
