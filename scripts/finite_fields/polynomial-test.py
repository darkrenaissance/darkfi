from __future__ import division
from test import test
from fractions import Fraction
from polynomial import *

from modp import *

Mod5 = IntegersModP(5)
Mod11 = IntegersModP(11)

polysOverQ = polynomialsOver(Fraction).factory
polysMod5 = polynomialsOver(Mod5).factory
polysMod11 = polynomialsOver(Mod11).factory

for p in [polysOverQ, polysMod5, polysMod11]:
   # equality
   test(True, p([]) == p([]))
   test(True, p([1,2]) == p([1,2]))
   test(True, p([1,2,0]) == p([1,2,0,0]))

   # addition
   test(p([1,2,3]), p([1,0,3]) + p([0,2]))
   test(p([1,2,3]), p([1,2,3]) + p([]))
   test(p([5,2,3]), p([4]) + p([1,2,3]))
   test(p([1,2]), p([1,2,3]) + p([0,0,-3]))

   # subtraction
   test(p([1,-2,3]), p([1,0,3]) - p([0,2]))
   test(p([1,2,3]), p([1,2,3]) - p([]))
   test(p([-1,-2,-3]), p([]) - p([1,2,3]))

   # multiplication
   test(p([1,2,1]), p([1,1]) * p([1,1]))
   test(p([2,5,5,3]), p([2,3]) * p([1,1,1]))
   test(p([0,7,49]), p([0,1,7]) * p([7]))

   # division
   test(p([1,1,1,1,1,1]), p([-1,0,0,0,0,0,1]) / p([-1,1]))
   test(p([-1,1,-1,1,-1,1]), p([1,0,0,0,0,0,1]) / p([1,1]))
   test(p([]), p([]) / p([1,1]))
   test(p([1,1]), p([1,1]) / p([1]))
   test(p([1,1]), p([2,2]) / p([2]))

   # modulus
   test(p([]), p([1,7,49]) % p([7]))
   test(p([-7]), p([-3,10,-5,3]) % p([1,3]))


test(polysOverQ([Fraction(1,7), 1, 7]), polysOverQ([1,7,49]) / polysOverQ([7]))
test(polysMod5([1 / Mod5(7), 1, 7]), polysMod5([1,7,49]) / polysMod5([7]))
test(polysMod11([1 / Mod11(7), 1, 7]), polysMod11([1,7,49]) / polysMod11([7]))
