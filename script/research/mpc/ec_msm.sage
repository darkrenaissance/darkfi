load('beaver.sage')


import random

N = 2

def sum_shares(shares, source, party_id):
    zero_share = AuthenticatedShare(0, source, party_id)
    for share in shares:
        zero_share += share
    return zero_share


source = Source(p)
points = [CurvePoint.random() for _ in range(0, N)]
scalars = [random.randint(0,p) for i in range(0, N)]

expected_msm = sum([p*s for p, s in zip(points, scalars)])

lhs_points = [pt - CurvePoint.random() for pt in points]
rhs_points = [point - lhs for (point, lhs) in zip(points, lhs_points)]
lhs_points_shares = [ECAuthenticatedShare(pt) for pt in lhs_points]
rhs_points_shares = [ECAuthenticatedShare(pt) for pt in rhs_points]
assert [lhs_pt_share.authenticated_open(rhs_pt_share)  for lhs_pt_share, rhs_pt_share in zip(lhs_points_shares, rhs_points_shares)] == points


lhs_scalars = [s - random.randint(0,p) for s in scalars]
rhs_scalars = [s - lhs for (s, lhs) in zip(scalars, lhs_scalars)]
lhs_scalars_shares = [AuthenticatedShare(s, source, 0) for s in lhs_scalars]
rhs_scalars_shares = [AuthenticatedShare(s, source, 1) for s in rhs_scalars]
assert [lhs_scalar_share.authenticated_open(rhs_scalar_share) for lhs_scalar_share, rhs_scalar_share in zip(lhs_scalars_shares, rhs_scalars_shares)] == scalars

## sas
#print('lhs sas')
lhs_sas_shares = [ScalingECAuthenticatedShares(lhs_points_share, rhs_scalars_share, source.triplet(0), 0) for lhs_points_share, rhs_scalars_share in zip(lhs_points_shares, lhs_scalars_shares)]
#print('rhs sas')
rhs_sas_shares = [ScalingECAuthenticatedShares(rhs_points_share, rhs_scalars_share, source.triplet(1), 1) for rhs_points_share, rhs_scalars_share in zip(rhs_points_shares, rhs_scalars_shares)]
#
lhs_sas_de = [[lhs_sas_i.d.copy(), lhs_sas_i.e.copy()] for lhs_sas_i in lhs_sas_shares]
print("lhs_sas_de: {}".format(lhs_sas_de))
rhs_sas_de = [[rhs_sas_i.d.copy(), rhs_sas_i.e.copy()] for rhs_sas_i in rhs_sas_shares]

lhs_sas = [lhs_sas_i.mul(rhs_sas_de_i[0], rhs_sas_de_i[1]) for lhs_sas_i, rhs_sas_de_i in zip(lhs_sas_shares, rhs_sas_de)]
rhs_sas = [rhs_sas_i.mul(lhs_sas_de_i[0], lhs_sas_de_i[1]) for rhs_sas_i, lhs_sas_de_i in zip(rhs_sas_shares, lhs_sas_de)]
mul_sas = [lhs_i.authenticated_open(rhs_i) for lhs_i, rhs_i in zip(lhs_sas, rhs_sas)]

assert sum(mul_sas) == expected_msm

## msm
#print('lhs msm')
lhs_msm = MSM(lhs_points_shares, lhs_scalars_shares, source, 0)
#print('rhs msm')
rhs_msm = MSM(rhs_points_shares, rhs_scalars_shares, source, 1)
lhs_msm_de = [[point_scalar.d, point_scalar.e] for point_scalar in lhs_msm.point_scalars]
print("lhs_msm_de: {}".format(lhs_msm_de))
rhs_msm_de = [[point_scalar.d, point_scalar.e] for point_scalar in rhs_msm.point_scalars]

res = []
lhs = lhs_msm.msm(rhs_msm_de)
rhs = rhs_msm.msm(lhs_msm_de)

#assert lhs_sas == lhs_msm.point_scalars, print('sas: {}, msm: {}'.format(lhs_sas, lhs_msm.point_scalars))
#assert rhs_sas == rhs_msm.point_scalars, print('sas: {}, msm: {}'.format(rhs_sas, rhs_msm.point_scalars))

print('msm: {}'.format(lhs_msm.point_scalars))
print('sas: {}'.format(lhs_sas))

result = sum([lhs_pt_scalar.authenticated_open(rhs_pt_scalar) for lhs_pt_scalar, rhs_pt_scalar in zip (lhs_msm.point_scalars , rhs_msm.point_scalars)])
res = lhs.authenticated_open(rhs)
assert result == res
assert res == expected_msm, 'res: {}, expected: {}'.format(res, expected_msm)
