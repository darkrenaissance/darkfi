from modp import *
from test import test

mod7 = IntegersModP(7)

test(mod7(5), mod7(5)) # Sanity check
test(mod7(5), 1 / mod7(3))
test(mod7(1), mod7(3) * mod7(5)) 
test(mod7(3), mod7(3) * 1)
test(mod7(2), mod7(5) + mod7(4))

test(True, mod7(0) == mod7(3) + mod7(4))

