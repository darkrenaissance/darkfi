# First run div2.sage to generate the function and points which
# are loaded below.

p = 115792089237316195423570985008687907853269984665640564039457584007908834671663
r = 115792089237316195423570985008687907852837564279074904382605163141518161494337
Fp = GF(p)  # Base Field
Fr = GF(r)  # Scalar Field
A = 0
B = 7
E = EllipticCurve(GF(p), [A, B])
assert(E.cardinality() == r)

K.<x> = PolynomialRing(Fp, implementation="generic")
L.<y> = PolynomialRing(K, implementation="generic")
M.<z> = L[]
eqn = y^2 - x^3 - A * x - B

def slope_intercept(P1, P2):
    P1x, P1y = P1.xy()
    P2x, P2y = P2.xy()
    P3x, P3y = (-(P1 + P2)).xy()
    λ = (P2y - P1y) / (P2x - P1x)
    μ = P2y - λ*P2x
    return λ, μ

A0 = E.random_element()
A1 = E.random_element()
A2 = -(A0 + A1)

[P0, P1, P2, Q, f] = load("div.sobj")

λ, μ = slope_intercept(A0, A1)
g = y - λ*x - μ

class Func:
    def __init__(self, func):
        self.func = func

    def __call__(self, P):
        Px, Py = P.xy()
        return self.func(x=Px, y=Py)

f = Func(f)
g = Func(g)

# We can actually compute this check in a more efficient way
assert f(A0) * f(A1) * f(A2) == -g(P0) * g(P1)^2 * g(P2)^3 * g(Q)^5

def dlog(D):
    Dx = D.differentiate(x)
    Dy = D.differentiate(y)
    #Dz = Dx + Dy * ((3*x^2 + A) / (2*y))

    # Normally we calculate:
    #   Dz/D
    # Due to a bug in sage, we will make the denominator D
    # solely an equation in x by taking its norm.

    # Denominator = V · V'
    V = 2*y * D
    # 2y Dz
    Dz_numer = ( (2*y*Dx + Dy * (3*x^2 + A)) * V(y=-y) ).mod(eqn)
    # Change denominator to the norm
    D_denom = (V * V(y=-y)).mod(eqn)

    return Dz_numer / D_denom

# Just confirm results from the paper
assert λ^2 == A0[0] + A1[0] + A2[0]

dy_dx = (3*x^2 + A)/(2*y)
dx_dz = 1/(dy_dx - λ)

dx_dz = Func(dx_dz)
assert dx_dz(A0) + dx_dz(A1) + dx_dz(A2) == 0

# These are the actual checks
Dlog = dlog(f.func)
# Another way to calculate dlog:
#D = f.func
#a_X = K(D(y=0))
#b_X = K(D(y=1) - a_X)
#assert D == a_X + y*b_X
#diff_f = a_X.differentiate(x) + dy_dx*b_X + y*b_X.differentiate(x)
#assert diff_f == D.differentiate(x) + D.differentiate(y)*dy_dx
#diff_f = Func(diff_f)

F = Func(Dlog * dx_dz.func)
G = Func(-1/g.func)

# The prover constructs a proof of this relation being true
assert F(A0) + F(A1) + F(A2) == G(P0) + 2*G(P1) + 3*G(P2) + 5*G(Q)

