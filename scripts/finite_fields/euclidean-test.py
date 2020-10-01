from test import test
from euclidean import *

test(1, gcd(7, 9))
test(2, gcd(8, 18))
test(-12, gcd(-12, 24))
test(12, gcd(12, -24)) # gcd is only unique up to multiplication by a unit, and so sometimes we'll get negatives.
test(38, gcd(4864, 3458))

test((32, -45, 38), extendedEuclideanAlgorithm(4864, 3458))
test((-45, 32, 38), extendedEuclideanAlgorithm(3458, 4864))

from modp import *

Mod2 = IntegersModP(2)
test(Mod2(1), gcd(Mod2(1), Mod2(0)))
test(Mod2(1), gcd(Mod2(1), Mod2(1)))
test(Mod2(0), gcd(Mod2(2), Mod2(2)))

Mod7 = IntegersModP(7)
test(Mod7(6), gcd(Mod7(6), Mod7(14)))
test(Mod7(2), gcd(Mod7(6), Mod7(9)))

ModHuge = IntegersModP(9923)
test(ModHuge(38), gcd(ModHuge(4864), ModHuge(3458)))
test((ModHuge(32), ModHuge(-45), ModHuge(38)),
     extendedEuclideanAlgorithm(ModHuge(4864), ModHuge(3458)))

from polynomial import *

p = polynomialsOver(Mod7).factory
test(p([-1, 1]), gcd(p([-1,0,1]), p([-1,0,0,1])))
f = p([-1,0,1])
g = p([-1,0,0,1])
test((p([0,-1]), p([1]), p([-1, 1])), extendedEuclideanAlgorithm(f, g))
test(p([-1,1]), f * p([0,-1]) + g * p([1]))

p = polynomialsOver(Mod2).factory
f = p([1,0,0,0,1,1,1,0,1,1,1]) # x^10 + x^9 + x^8 + x^6 + x^5 + x^4 + 1
g = p([1,0,1,1,0,1,1,0,0,1])   # x^9 + x^6 + x^5 + x^3 + x^1 + 1
theGcd = p([1,1,0,1]) # x^3 + x + 1
x = p([0,0,0,0,1]) # x^4
y = p([1,1,1,1,1,1]) # x^5 + x^4 + x^3 + x^2 + x + 1

test((x, y, theGcd), extendedEuclideanAlgorithm(f, g))
