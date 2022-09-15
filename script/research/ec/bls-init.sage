q = 0x1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab
F1 = GF(q)
# F₂ is constructed as F(u) / (u² + 1)
# F₆ is constructed as F₂(v) / (v³- (u + 1))
# F₁₂ is constructed as F₆(w) / (w²- v)

# we can't do extension field towers in sage...
# https://ask.sagemath.org/question/49663/efficiently-computing-tower-fields-for-pairings/

K2.<x> = PolynomialRing(F1)
F2.<u> = F1.extension(x^2 + 1)

# The last line in this section will hang forever
#K6.<y> = PolynomialRing(F2)
#F6.<v> = F2.extension(y^3 - (u + 1))
#K12.<z> = PolynomialRing(F6)
#F12.<w> = F6.extension(z^2 - v)

# Alternative construction
R.<y> = PolynomialRing(F2)
# w is a root of a(y) = y^6 - (u + 1) and also b(y) = y^2 - v
# v is a root of c(y) = y^3 - (u + 1), so to enlarge F2 -> F12, we use a(y)
F12.<w> = F2.extension(y^6 - (u + 1))
v = w^2

assert u^2 + 1 == 0
assert v^3 - (u + 1) == 0
assert w^2 - v == 0

