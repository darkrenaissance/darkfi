load('share.sage')
load('beaver.sage')
import random
import numpy as np

party0_val = [1,2]
party1_val = [2,4]
source = Source(p)

party0_random = [1,1]
party1_random = [1,1]

# additive share distribution, and communication of private values

# party 0 shares
alpha1_l = [AuthenticatedShare(party0_random[i], source, 0) for i in range(2)]
beta1_l = [AuthenticatedShare(party1_random[i], source, 1) for i in range(2)]
# party 1 shares
alpha2_l = [AuthenticatedShare(party0_val[i] - party0_random[i], source, 0) for i in range(2)]
beta2_l = [AuthenticatedShare(party1_val[i] - party1_random[i], source, 1) for i in range(2)]

# party 0 c
a1b1_l = [MultiplicationAuthenticatedShares(alpha1, beta1, source.triplet(0), 0) for alpha1, beta1 in zip(alpha1_l, beta1_l)]
# party 1 c
a2b2_l = [MultiplicationAuthenticatedShares(alpha2, beta2, source.triplet(1), 1) for alpha2, beta2 in zip(alpha2_l, beta2_l)]
# party 0 de
for a1b1 in a1b1_l:
    print('a1b1: d/e: {}/{}'.format(a1b1.d, a1b1.e))
# party 1 de
for a2b2 in a2b2_l:
    print('a2b2: d/e: {}/{}'.format(a2b2.d, a2b2.e))

lhs_l = [a1b1.mul(a2b2.d, a2b2.e) for a1b1, a2b2 in zip(a1b1_l, a2b2_l)]
rhs_l = [a2b2.mul(a1b1.d, a1b1.e) for a1b1, a2b2 in zip(a1b1_l, a2b2_l)]
res = [lhs.authenticated_open(rhs) for lhs, rhs in zip(lhs_l, rhs_l)]
print('c: {}'.format(sum(res)))
assert (sum(res) == np.dot(party0_val,party1_val)), 'mul: {}, expected mul: {}'.format(res, party0_val*party1_val)
