load('share.sage')
load('beaver.sage')
from random import randint


party0_val = 3
party1_val = 22
public_scalar = 2
source = Source(p)

# additive share distribution, and communication of private values
party0_random = randint(0,p)
alpha1 = AuthenticatedShare(party0_random, source, 0)
alpha2 = AuthenticatedShare(party0_val - party0_random, source, 1)
assert (alpha1.authenticated_open(alpha2) == party0_val)

party1_random = randint(0,p)
beta1 = AuthenticatedShare(party1_random, source, 1)
beta2 = AuthenticatedShare(party1_val - party1_random, source, 1)
assert (beta1.authenticated_open(beta2) == party1_val)

# mul_scalar by public scalar
mul_left_share = alpha1.mul_scalar(public_scalar)
mul_right_share = alpha2.mul_scalar(public_scalar)
assert (mul_left_share.authenticated_open(mul_right_share) == (public_scalar * party0_val))


# sub_scalar by public scalar
sub_left_share = alpha1.sub_scalar(public_scalar, 0)
sub_right_share = alpha2.sub_scalar(public_scalar, 1)
assert (sub_left_share.authenticated_open(sub_right_share) == (party0_val - public_scalar))



# add_scalar by public scalar
add_left_share = alpha1.add_scalar(public_scalar, 0)
add_right_share = alpha2.add_scalar(public_scalar, 1)
assert (add_left_share.authenticated_open(add_right_share) == (public_scalar + party0_val))



# add authenticated shares
add_party0_share = alpha1 + beta2
add_party1_share = alpha2 + beta1

lhs = add_party0_share.authenticated_open(add_party1_share)

assert (lhs == (party0_val + party1_val))


# sub authenticated shares
sub_party0_share = alpha1 - beta2
sub_party1_share = alpha2 - beta1

lhs = sub_party0_share.authenticated_open(sub_party1_share)

assert (lhs == (party0_val - party1_val))



# mul authenticated shares
mul_res = party0_val * party1_val

s = Source(p)
a1b1 = MultiplicationAuthenticatedShares(alpha1, beta1, s.triplet(0), 0)
a2b2 = MultiplicationAuthenticatedShares(alpha2, beta2, s.triplet(1), 1)
print('d1: {}'.format(a1b1.d))
print('d2: {}'.format(a2b2.d))
lhs_share = a1b1.mul(a2b2.d, a2b2.e)
rhs_share = a2b2.mul(a1b1.d, a1b1.e)
lhs = lhs_share.authenticated_open(rhs_share)

assert (lhs == (party0_val * party1_val)), 'lhs: {}, rhs: {}'.format(lhs, party0_val * party1_val)
