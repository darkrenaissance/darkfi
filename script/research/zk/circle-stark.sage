def random_mersenne_prime():
    while True:
        p = random_prime(100, 200)
        m = 2^p - 1
        if is_prime(m):
            return m

#p = random_mersenne_prime()
p = 8191

# -1 should not be a quadratic residue modulo p
assert legendre_symbol(-1, p) == -1
assert p % 4 == 3

F = GF(p)
R.<x> = F[]
K.<i> = F.extension(x^2 + 1)

def get_point(t):
    x = (1 - t^2)/(1 + t^2)
    y = 2*t / (1 + t^2)
    return (x, y)

(p1_x, p1_y) = get_point(K(3))
assert p1_x^2 + p1_y^2 == 1
z = p1_x + i*p1_y

(p2_x, p2_y) = get_point(K(7))
assert p2_x^2 + p2_y^2 == 1
w = p2_x + i*p2_y

def abs(z):
    return z[0]^2 + z[1]^2

# z and w are now elements of F_p(i) which is a field
assert abs(z) == 1
assert abs(w) == 1
assert abs(z * w) == 1

# We can also construct the inverse
def conjugate(z):
    return z[0] - i*z[1]
# Remember that (x + iy)(x - iy) = x^2 - i^2 y^2 = x^2 + y^2
assert z * conjugate(z) in F
# Now we find the multiplicative inverse
z_inv = conjugate(z) / (z * conjugate(z))
assert z * z_inv == 1

# Size of K is p^2
assert len(K) == p^2

# Because in sage we cannot construct a homomorphism to GF(p^2) directly
# we instead construct the isomorphic field extension, and use that instead.
Fp2.<a> = GF(p^2)
conway_fp2 = a.minimal_polynomial()
L.<j> = F.extension(conway_fp2(x=x))

y = L.multiplicative_generator()
phi = K.hom([y^(y.multiplicative_order()/i.multiplicative_order())])
# K ≌ GF(p^2)
assert phi.is_injective() and phi.is_surjective()

assert a.multiplicative_order() == p^2 - 1
g_K = phi.inverse()(y)
g1 = g_K^int((p^2 - 1)/(p + 1))
assert abs(g1) == 1

g2 = K.multiplicative_generator()
g2 = g2^(p - 1)
assert abs(g2) == 1

# bug in sage where .unit_group is missing
# https://ask.sagemath.org/question/62822/make-morphism-from-gfp2s-multiplicative-group-to-gfps-multiplicative-group/
C = AbelianGroup([p + 1])
g, = C.gens()
while True:
    print(f"Group of order = {g.order()}")
    for C2 in C.subgroups():
        print(f"  {C2}")
    print()

    if g.order() == 1:
        break

    # For some annoying reason, I cannot iterate on subgroups
    # C = C.subgroup([g^2])
    # Trying to get the subgroup of this will give me some error.
    # Lets just construct it manually.
    C = AbelianGroup([(g^2).order()])
    g, = C.gens()

