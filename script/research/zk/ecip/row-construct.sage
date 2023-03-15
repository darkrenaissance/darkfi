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

B0 = E.random_element()
B1 = E.random_element()

# Base 3 representation
d0 = [ 1, -1, 0,  0, 0]
d1 = [-1, -1, 0, -1, 1]

e0 = sum(d0_j*(-3)^j for j, d0_j in enumerate(d0))
assert e0 == 4

e1 = sum(d1_j*(-3)^j for j, d1_j in enumerate(d1))
assert e1 == 110

# We will prove this statement
Q = 4*B0 + 110*B1
assert Q == (
    (-3)^0 * ( B0 - B1) +
    (-3)^1 * (-B0 - B1) +

    (-3)^3 * (-B1) +
    (-3)^4 * (B1)
)

Q5 = E(0, 1, 0)
Q4 = -3*Q5 + B1
Q3 = -3*Q4 - B1
Q2 = -3*Q3
Q1 = -3*Q2 - B0 - B1
Q0 = -3*Q1 + B0 - B1
assert Q0 == Q

a0 = (-3)^0
b0 = (-3)^1
assert e0 == a0 - b0

a1 = (-3)^4
b1 = (-3)^0 + (-3)^1 + (-3)^3
assert e1 == a1 - b1

