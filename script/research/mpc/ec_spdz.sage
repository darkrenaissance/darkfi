load('beaver.sage')
load('curve.sage')
load('ec_share.sage')


party0_val = CurvePoint.random()
party1_val = CurvePoint.random()
public_scalar = 2
source = Source(p)
# additive share distribution, and communication of private values
party0_random = CurvePoint.random()
alpha1 = ECAuthenticatedShare(party0_random)
alpha2 = ECAuthenticatedShare(party0_val - party0_random)
assert (alpha1.authenticated_open(alpha2) == party0_val)

party1_random = CurvePoint.random()
beta1 = ECAuthenticatedShare(party1_random)
beta2 = ECAuthenticatedShare(party1_val - party1_random)
assert (beta1.authenticated_open(beta2) == party1_val)

# mul_scalar by public scalar
mul_left_share = alpha1.mul_scalar(public_scalar)
mul_right_share = alpha2.mul_scalar(public_scalar)
assert (mul_left_share.authenticated_open(mul_right_share) == (public_scalar * party0_val))

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


# authenticated ec point scaled with authenticated scalar
party1_val = random.randint(0,p)
party1_random = random.randint(0,p)
beta1 = AuthenticatedShare(party1_random, source, 0)
beta2 = AuthenticatedShare(party1_val - party1_random, source, 0)
# scaling elliptic curve Authenticated share by authenticated scalar
a1b1 = ScalingECAuthenticatedShares(alpha1, beta1, source.triplet(0), 0)
a2b2 = ScalingECAuthenticatedShares(alpha2, beta2, source.triplet(1), 1)

lhs = a1b1.mul(a2b2.d, a2b2.e)
rhs = a2b2.mul(a1b1.d, a1b1.e)
res = lhs.authenticated_open(rhs)

mul_res = party0_val * party1_val
assert (res == (party0_val * party1_val)), 'lhs: {}, rhs: {}'.format(res, party0_val * party1_val)
