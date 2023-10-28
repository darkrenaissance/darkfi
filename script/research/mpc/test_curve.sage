load('curve.sage')
import random

pt = CurvePoint.random()
rnd = random.randint(0, p)
s_ff = K(rnd)
s = int(s_ff)
assert s == rnd
s_inv_ff = 1/s_ff
s_inv = int(s_inv_ff)
assert K(s*s_inv) == K(1)
assert (pt * int(K(s * s_inv))) == pt
