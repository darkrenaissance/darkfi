load('beaver.sage')

import random

N = 10

source = Source(p)
points = [CurvePoint.random() for _ in range(0, N)]

lhs_points = [pt - CurvePoint.random() for pt in points]
rhs_points = [point - lhs for (point, lhs) in zip(points, lhs_points)]
lhs_points_shares = [ECAuthenticatedShare(pt) for pt in lhs_points]
rhs_points_shares = [ECAuthenticatedShare(pt) for pt in rhs_points]


scalars = [random.randint(0,p) for i in range(0, N)]
lhs_scalars = [s - random.randint(0,p) for s in scalars]
rhs_scalars = [s - lhs for (s, lhs) in zip(scalars, lhs_scalars)]
lhs_scalars_shares = [AuthenticatedShare(s, source, 0) for s in lhs_scalars]
rhs_scalars_shares = [AuthenticatedShare(s, source, 1) for s in rhs_scalars]

lhs_msm = MSM(lhs_points_shares, lhs_scalars_shares, source, 0)
rhs_msm = MSM(rhs_points_shares, rhs_scalars_shares, source, 1)

#
lhs_de = [[point_scalar.d, point_scalar.e] for point_scalar in lhs_msm.point_scalars]
rhs_de = [[point_scalar.d, point_scalar.e] for point_scalar in rhs_msm.point_scalars]
res = []
lhs = lhs_msm.msm(rhs_de)
rhs = rhs_msm.msm(lhs_de)
res = lhs.authenticated_open(rhs)

assert res == sum([p*s for p, s in zip(points, scalars)]), 'res: {}, expected: {}'.format(res, sum([p*s for p, s in zip(points, scalars)]))
