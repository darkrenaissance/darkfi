load('beaver.sage')
load('curve.sage')
load('ec_share.sage')

p = 10

party0_val = CurvePoint.random()
party1_val = CurvePoint.random()
public_scalar = 2

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
beta1 = AuthenticatedShare(party1_random)
beta2 = AuthenticatedShare(party1_val - party1_random)

s = Source(p)
alpha1beta1_share = ScalingECAuthenticatedShares(alpha1, beta1, s.triplet(0), 0)
alpha2beta2_share = ScalingECAuthenticatedShares(alpha2, beta2, s.triplet(1), 1)

lhs_share = alpha1beta1_share * alpha2beta2_share
rhs_share = alpha2beta2_share * alpha1beta1_share
lhs = lhs_share.authenticated_open(rhs_share)

mul_res = party0_val * party1_val
assert (lhs == (party0_val * party1_val)), 'lhs: {}, rhs: {}'.format(lhs, party0_val * party1_val)
