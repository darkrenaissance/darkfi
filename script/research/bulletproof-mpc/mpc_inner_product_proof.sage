load('../mpc/curve.sage')
load('proof.sage')
load('transcript.sage')
load('../mpc/beaver.sage')
load('proof_mpc.sage')
import gc
import numpy as np

##
n = 2
#Q = to_ec_shares_list([CurvePoint.generator()])
#H = to_ec_shares_list([CurvePoint.generator() for i in range(0,n)])
#G = to_ec_shares_list([CurvePoint.generator() for i in range(0,n)])
Q = to_ec_shares_list([CurvePoint.generator()])
Q1 = to_ec_shares_list([CurvePoint.random()])
Q2 = [q - q1 for q, q1 in zip(Q, Q1)]

H = to_ec_shares_list([CurvePoint.generator() for i in range(0,n)])
H1 = to_ec_shares_list([CurvePoint.random() for i in range(0,n)])
H2 = [h - h1 for h, h1 in zip(H, H1)]

G = to_ec_shares_list([CurvePoint.generator() for i in range(0,n)])
G1 = to_ec_shares_list([CurvePoint.random() for i in range(0,n)])
G2 = [g - g1 for g, g1 in zip(G, G1)]

## source
source = TestSource()
## alpha
party0_val = [1,2]
party1_val = [2,4]
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

# validate L_gr_al_g
L_gr_al_g_truth_table = [874739451078007766457464989774322083649278607533249481151382481072868806602, 152666792071518830868575557812948353041420400780739481342941381225525861407 , 1]
L_gr_al_g_lhs = party_0_proof.L_gr_al_g_share.copy().msm(party_1_proof.L_gr_al_g_share.copy().de())
L_gr_al_g_rhs = party_1_proof.L_gr_al_g_share.copy().msm(party_0_proof.L_gr_al_g_share.copy().de())
L_gr_al_g = L_gr_al_g_lhs.authenticated_open(L_gr_al_g_rhs)
assert L_gr_al_g[0] == L_gr_al_g_truth_table[0] and L_gr_al_g[1] == L_gr_al_g_truth_table[1], 'L_gr_al_g: {}'.format(L_gr_al_g)

# validate L_hl_br_h
L_hl_br_h_truth_table = [296568192680735721663075531306405401515803196637037431012739700151231900092, 2496008012906462030584867856951610048657271546413643307709739611216909709750, 1]
L_hl_br_h_lhs = party_0_proof.L_hl_br_h_share.copy().msm(party_1_proof.L_hl_br_h_share.copy().de())
L_hl_br_h_rhs = party_1_proof.L_hl_br_h_share.copy().msm(party_0_proof.L_hl_br_h_share.copy().de())
L_hl_br_h = L_hl_br_h_lhs.authenticated_open(L_hl_br_h_rhs)
assert L_hl_br_h[0] == L_hl_br_h_truth_table[0] and L_hl_br_h[1] == L_hl_br_h_truth_table[1], 'L_hl_br_h: {}'.format(L_hl_br_h)

# validate L_q_cl
L_q_cl_truth_table = [296568192680735721663075531306405401515803196637037431012739700151231900092, 2496008012906462030584867856951610048657271546413643307709739611216909709750, 1]
L_q_cl_lhs = party_0_proof.L_q_cl_share.copy().msm(party_1_proof.L_q_cl_share.copy().de())
L_q_cl_rhs = party_1_proof.L_q_cl_share.copy().msm(party_0_proof.L_q_cl_share.copy().de())

L_q_cl = L_q_cl_lhs.authenticated_open(L_q_cl_rhs)

assert L_q_cl[0] == L_q_cl_truth_table[0] and L_q_cl[1] == L_q_cl_truth_table[1], 'L_q_cl: {}'.format(L_q_cl)

#validate R_gl_ar_g
R_gl_ar_g_truth_table = [3324833730090626974525872402899302150520188025637965566623476530814354734325, 3147007486456030910661996439995670279305852583596209647900952752170983517249, 1]
R_gl_ar_g_lhs = party_0_proof.R_gl_ar_g_share.copy().msm(party_1_proof.R_gl_ar_g_share.copy().de())
R_gl_ar_g_rhs = party_1_proof.R_gl_ar_g_share.copy().msm(party_0_proof.R_gl_ar_g_share.copy().de())
print(R_gl_ar_g_lhs)
print(R_gl_ar_g_rhs)
R_gl_ar_g = R_gl_ar_g_lhs.authenticated_open(R_gl_ar_g_rhs)
assert R_gl_ar_g[0] == R_gl_ar_g_truth_table[0] and R_gl_ar_g[1] == R_gl_ar_g_truth_table[1], 'R_gl_ar_g: {}'.format(R_gl_ar_g)

#validate R_hr_bl_h
R_hr_bl_h_truth_table = [3324833730090626974525872402899302150520188025637965566623476530814354734325, 3147007486456030910661996439995670279305852583596209647900952752170983517249, 1]
R_hr_bl_h_lhs = party_0_proof.R_hr_bl_h_share.copy().msm(party_1_proof.R_hr_bl_h_share.copy().de())
R_hr_bl_h_rhs = party_1_proof.R_hr_bl_h_share.copy().msm(party_0_proof.R_hr_bl_h_share.copy().de())
R_hr_bl_h = R_hr_bl_h_lhs.authenticated_open(R_hr_bl_h_rhs)
assert R_hr_bl_h[0] == R_hr_bl_h_truth_table[0] and R_hr_bl_h[1] == R_hr_bl_h_truth_table[1], 'R_hr_bl_h: {}'.format(R_hr_bl_h)

#validate R_q_cr
R_q_cr_truth_table = [296568192680735721663075531306405401515803196637037431012739700151231900092, 2496008012906462030584867856951610048657271546413643307709739611216909709750, 1]
R_q_cr_lhs = party_0_proof.R_q_cr_share.copy().msm(party_1_proof.R_q_cr_share.copy().de())
R_q_cr_rhs = party_1_proof.R_q_cr_share.copy().msm(party_0_proof.R_q_cr_share.copy().de())
R_q_cr = R_q_cr_lhs.authenticated_open(R_q_cr_rhs)
print(R_q_cr_lhs)
print(R_q_cr_rhs)
assert R_q_cr[0] == R_q_cr_truth_table[0] and R_q_cr[1] == R_q_cr_truth_table[1], 'R_q_cr: {}'.format(R_q_cr)

# validate L,R
L_truth_table = [944745129853146482146311827146531433242387523423467361347719369673366386761, 2394221861052833782597287772330532919046009427329165562185942323334687758988, 1]
R_truth_table = [3136030469135674343172465880817263454880219855664441593466904169223571314065, 3230850854683103635133032411878658931556916918508772276704988424959453909526, 1]
L = []
R = []
for party_0_proof_lhs, party_1_proof_lhs in zip(party_0_proof.lhs, party_1_proof.lhs):
    l_i_l = []
    for i in range(len(party_0_proof_lhs)):
        l_i_l_0_lhs = party_0_proof_lhs[i].copy().msm(party_1_proof_lhs[i].de())
        l_i_l_1_rhs = party_1_proof_lhs[i].copy().msm(party_0_proof_lhs[i].de())
        l_i_l += [l_i_l_0_lhs.authenticated_open(l_i_l_1_rhs)]
    L += [sum(l_i_l)]
assert L[0][0] == L_truth_table[0] and L[0][1] == L_truth_table[1], 'L: {}'.format(L)

for party_0_proof_rhs, party_1_proof_rhs in zip(party_0_proof.rhs, party_1_proof.rhs):
    r_i_l = []
    for i in range(len(party_0_proof_rhs)):
        r_i_l_lhs = party_0_proof_rhs[i].copy().msm(party_1_proof_rhs[i].de())
        r_i_l_rhs = party_1_proof_rhs[i].copy().msm(party_0_proof_rhs[i].de())
        r_i_l += [r_i_l_lhs.authenticated_open(r_i_l_rhs)]
    R += [sum(r_i_l)]
assert R[0][0] == R_truth_table[0] and R[0][1] == R_truth_table[1], 'R: {}'.format(R)

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
