# Boolean hypercube means a bitstring
# 3d boolean hypercube = ℤ₂³
var("x y z")

f = 3*x*y + 5*x*z + 2*x*y*z + 6

claimed_eval = sum([
    f(x=0, y=0, z=0),
    f(x=0, y=0, z=1),
    f(x=0, y=1, z=0),
    f(x=0, y=1, z=1),
    f(x=1, y=0, z=0),
    f(x=1, y=0, z=1),
    f(x=1, y=1, z=0),
    f(x=1, y=1, z=1),
])
# We will now prove the claim
assert claimed_eval == 66

# Prover constructs g1 such that g1(0) + g1(1) == claimed_eval
g1_0 = sum([
    f(x=0, y=0, z=0),
    f(x=0, y=0, z=1),
    f(x=0, y=1, z=0),
    f(x=0, y=1, z=1),
])
g1_1 = sum([
    f(x=1, y=0, z=0),
    f(x=1, y=0, z=1),
    f(x=1, y=1, z=0),
    f(x=1, y=1, z=1),
])
g1 = (1 - x)*g1_0 + x*g1_1

# Verifier:
assert g1(x=0) + g1(x=1) == claimed_eval
r1 = 2

# Prover now constructs g2(y) such that g1(r1) == g2(0) + g2(1)
g2_0 = sum([
    f(x=r1, y=0, z=0),
    f(x=r1, y=0, z=1),
])
g2_1 = sum([
    f(x=r1, y=1, z=0),
    f(x=r1, y=1, z=1),
])
g2 = (1 - y)*g2_0 + y*g2_1

# Verifier
assert g2(y=0) + g2(y=1) == g1(x=r1)
r2 = 7

# Prover constructs g3(z) : g2(r2) == g3(0) + g3(1)
g3_0 = f(x=r1, y=r2, z=0)
g3_1 = f(x=r1, y=r2, z=1)
g3 = (1 - z)*g3_0 + z*g3_1

# Now verifier picks a random challenge
α = 9
# and checks f(r1, r2, α) == g3(α)
assert f(x=r1, y=r2, z=α) == g3(z=α)

# The verifier is now convinced the claimed_eval is correct.
# They did not need to sum a whole load of evaluations.

