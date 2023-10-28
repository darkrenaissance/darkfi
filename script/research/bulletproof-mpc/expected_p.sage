load('../mpc/curve.sage')
load('proof.sage')
load('transcript.sage')
load('../mpc/beaver.sage')
load('proof_mpc.sage')
import gc
##
n = 2
Q = to_ec_shares_list([CurvePoint.generator()])
Q1 = to_ec_shares_list([CurvePoint.random()])
Q2 = [q - q1 for q, q1 in zip(Q, Q1)]
H = to_ec_shares_list([CurvePoint.generator() for i in range(0,n)])
H1 = to_ec_shares_list([CurvePoint.random() for i in range(0,n)])
H2 = [h - h1 for h, h1 in zip(H, H1)]
G = to_ec_shares_list([CurvePoint.generator() for i in range(0,n)])
G1 = to_ec_shares_list([CurvePoint.random() for i in range(0,n)])
G2 = [g - g1 for g, g1 in zip(G, G1)]

assert sum(g1.authenticated_open(g2)==CurvePoint.generator() for g1, g2 in zip(G1, G2)) == n

## source
source = Source(p)
## alpha
party0_val = [1, 2] # a
party1_val = [2, 4] # b
party0_random = [1, 1]
alpha1 = [AuthenticatedShare(party0_random[i], source, 0) for i in range(0,n)]
alpha2 = [AuthenticatedShare(party0_val[i] - party0_random[i], source, 0) for i in range(0,n)]
a_shares = [alpha1, alpha2]

## generators factors
y_inv = K(1)
G_factors = [K(1)]*n
H_factors = [y_inv**i for i in range(0,n)]
##
party_0_a_prime_shares = a_shares[0].copy()
party_1_a_prime_shares = a_shares[1].copy()
##
party_0_g_a_prime_shares = MSM(G1, party_0_a_prime_shares, source, 0)
party_1_g_a_prime_shares = MSM(G2, party_1_a_prime_shares, source, 1)
## msm multiplication shares announcement for g_a_prime_shares
party_0_g_a_prime_shares_de = [[party_0_g_a_prime_share.d, party_0_g_a_prime_share.e] for party_0_g_a_prime_share in party_0_g_a_prime_shares.point_scalars]
party_1_g_a_prime_shares_de = [[party_1_g_a_prime_share.d, party_1_g_a_prime_share.e] for party_1_g_a_prime_share in party_1_g_a_prime_shares.point_scalars]
party_0_g_a_prime_shares = party_0_g_a_prime_shares.msm(party_1_g_a_prime_shares_de)
party_1_g_a_prime_shares = party_1_g_a_prime_shares.msm(party_0_g_a_prime_shares_de)
g_a_prime = party_0_g_a_prime_shares.authenticated_open(party_1_g_a_prime_shares)
#a_primes = [h*a for h, a in zip(H_factors, party0_val)]
a_primes = party0_val.copy()
expected_g_a_prime = sum([CurvePoint.generator() * a_prime  for a_prime in a_primes])
assert (expected_g_a_prime == g_a_prime), 'expected_g_a_prime: {}, g_a_prime: {}'.format(expected_g_a_prime, g_a_prime)

## beta
party1_random = [1, 1]
beta1 = [AuthenticatedShare(party1_random[i], source, 1) for i in range(0,n)]
beta2 = [AuthenticatedShare(party1_val[i] - party1_random[i], source, 1) for i in range(0,n)]
b_shares = [beta1, beta2]
##
party_0_b_prime_shares = [b_share.mul_scalar(y) for b_share, y in  zip(b_shares[0], H_factors)]
party_1_b_prime_shares = [b_share.mul_scalar(y) for b_share, y in  zip(b_shares[1], H_factors)]

## c shares
my_c_shares = [MultiplicationAuthenticatedShares(a_share, b_share, source.triplet(0), 0) for a_share, b_share in zip(a_shares[0], b_shares[0])]
their_c_shares = [MultiplicationAuthenticatedShares(peer_a_share, peer_b_share, source.triplet(1), 1) for peer_a_share, peer_b_share in zip(a_shares[1], b_shares[1])]
party_0_c_shares = [my_c_share.mul(their_c_share.d, their_c_share.e) for my_c_share, their_c_share in zip(my_c_shares, their_c_shares)]
party_0_c_share = [sum_shares(party_0_c_shares, source, 0)]
party_0_q_c_shares = MSM(Q1, party_0_c_share, source, 0)
party_1_c_shares = [their_c_share.mul(my_c_share.d, my_c_share.e) for my_c_share, their_c_share in zip(my_c_shares, their_c_shares)]
party_1_c_share = [sum_shares(party_1_c_shares, source, 1)]
party_1_q_c_shares = MSM(Q2, party_1_c_share, source, 1)
c_shares = [party_0_c_share[0].authenticated_open(party_1_c_share[0])]
print('c: {}'.format(c_shares[0]))
assert(c_shares[0] == sum([a*b for a,b in zip(party0_val, party1_val)])), 'sum: {}'.format(sum([a*b for a,b in zip(party0_val, party1_val)]))

party_0_h_b_prime_shares = MSM(H1, party_0_b_prime_shares, source, 0)
party_1_h_b_prime_shares = MSM(H2, party_1_b_prime_shares, source, 1)
## msm multiplication shares announcement for g_b_prime_shares
party_0_h_b_prime_shares_de = [[party_0_h_b_prime_share.d, party_0_h_b_prime_share.e] for party_0_h_b_prime_share in party_0_h_b_prime_shares.point_scalars]
party_1_h_b_prime_shares_de = [[party_1_h_b_prime_share.d, party_1_h_b_prime_share.e] for party_1_h_b_prime_share in party_1_h_b_prime_shares.point_scalars]
party_0_h_b_prime_shares = party_0_h_b_prime_shares.msm(party_1_h_b_prime_shares_de)
party_1_h_b_prime_shares = party_1_h_b_prime_shares.msm(party_0_h_b_prime_shares_de)
h_b_prime = party_0_h_b_prime_shares.authenticated_open(party_1_h_b_prime_shares)
b_primes = [h*b for h, b in zip(H_factors, party1_val)]
expected_h_b_prime = sum([CurvePoint.generator() * b_prime for b_prime in b_primes])
assert (expected_h_b_prime == h_b_prime), 'expected_h_b_prime: {}, h_b_prime: {}'.format(expected_h_b_prime, h_b_prime)

## msm multiplication shares announcement for q_c_prime_shares
party_0_q_c_shares_de = [[party_0_q_c_share.d, party_0_q_c_share.e] for party_0_q_c_share in party_0_q_c_shares.point_scalars]
party_1_q_c_shares_de = [[party_1_q_c_share.d, party_1_q_c_share.e] for party_1_q_c_share in party_1_q_c_shares.point_scalars]
party_0_q_c_shares_lhs = party_0_q_c_shares.msm(party_1_q_c_shares_de)
party_1_q_c_shares_rhs = party_1_q_c_shares.msm(party_0_q_c_shares_de)
q_c = party_0_q_c_shares_lhs.authenticated_open(party_1_q_c_shares_rhs)

## expected P
expected_P =  sum([g_a_prime, h_b_prime, q_c])
truth_table = [2147917197054818871619776655514917967724810669246777137580480562218260377891, 1230877877612900447137853367185807507097371113825426166020962037710421986578, 1]

print('g_a_prime: {}'.format(g_a_prime))
print('h_b_prime: {}'.format(h_b_prime))
print('q_c: {}'.format(q_c))
assert expected_P[0] == truth_table[0] or expected_P[1] == truth_table[1], 'P: {}, truth_Table: {}'.format(expected_P, truth_table)
