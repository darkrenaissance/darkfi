load('share.sage')
load('beaver.sage')
import random

party0_val = 2
party1_val = 3
source = Source(p)

# additive share distribution, and communication of private values
party0_random = 1
alpha1 = AuthenticatedShare(party0_random, source, 0)
alpha2 = AuthenticatedShare(party0_val - party0_random, source, 0)
assert (alpha1.authenticated_open(alpha2) == party0_val)

party1_random = 1
beta1 = AuthenticatedShare(party1_random, source, 1)
beta2 = AuthenticatedShare(party1_val - party1_random, source, 1)
assert (beta1.authenticated_open(beta2) == party1_val)

a1b1 = MultiplicationAuthenticatedShares(alpha1, beta1, source.triplet(0), 0)
a2b2 = MultiplicationAuthenticatedShares(alpha2, beta2, source.triplet(1), 1)
print('alpha2: {}'.format(alpha2))
print('beta2: {}'.format(beta2))
print('d1: {}'.format(a1b1.d))
print('d2: {}'.format(a2b2.d))
print('e1: {}'.format(a1b1.e))
print('e2: {}'.format(a2b2.e))
lhs = a1b1.mul(a2b2.d, a2b2.e)
rhs = a2b2.mul(a1b1.d, a1b1.e)
res = lhs.authenticated_open(rhs)

assert (res == party0_val*party1_val), 'mul: {}, expected mul: {}'.format(res, party0_val*party1_val)
