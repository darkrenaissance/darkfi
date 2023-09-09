load('curve.sage')
load('share.sage')
load('ec_share.sage')
load('beaver.sage')

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
lhs_scalars_shares = [AuthenticatedShare(s) for s in lhs_scalars]
rhs_scalars_shares = [AuthenticatedShare(s) for s in rhs_scalars]

lhs_msm = MSM(lhs_points_shares, lhs_scalars_shares, source, 0)
rhs_msm = MSM(rhs_points_shares, rhs_scalars_shares, source, 1)

res = []
for lhs, rhs in  zip(lhs_msm.msm(), rhs_msm.msm()):
    first_share = lhs*rhs
    second_share = rhs*lhs
    res += [first_share.authenticated_open(second_share)]

assert (sum(res) == sum([p*s for p, s in zip(points, scalars)]))
