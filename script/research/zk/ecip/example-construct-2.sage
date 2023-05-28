load("div.sage")

# Initialize an elliptic curve
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

P1 = LabelPoint(E.random_element(), {"P₁": 1})
P2 = LabelPoint(E.random_element(), {"P₂": 1})
P3 = LabelPoint(E.random_element(), {"P₃": 1})
P4 = LabelPoint(E.random_element(), {"P₄": 1})
Q = -(P1.P + P2.P + P3.P + P4.P)
Q = LabelPoint(Q, {"Q": 1})
assert P1.P + P2.P + P3.P + P4.P + Q.P == E(0)

# Challenge line
A0 = LabelPoint(E.random_element(), {"A₀": 1})
A1 = LabelPoint(E.random_element(), {"A₁": 1})
X1 = div_line(A0, A1)

# First loop in construct
L1 = div_line(P1, P2)
Q1 = P1 + P2

L2 = div_line(P3, P4)
Q2 = P3 + P4

L3 = div_line(Q, -Q)
Q3 = Q
#print(f"L₁ = {L1}")
#print(f"L₂ = {L2}")
#print(f"L₃ = {L3}")

divs = [L1, L2, L3]

# Now apply reduction algo

# len(divs) == 3

D1 = L1
Q1 = Q1

# i = 0

ℓ = div_line(Q2, Q3)
D2 = ℓ + L2 + L3 - div_line(Q2, -Q2) - div_line(Q3, -Q3)
Q2 = Q2 + Q3

divs = [D1, D2]

# len(divs) == 2

ℓ = div_line(Q1, Q2)
D1 = ℓ + D1 + D2 - div_line(Q1, -Q1) - div_line(Q2, -Q2)
Q1 = Q1 + Q2

divs = [D1]
D = D1

assert D.is_equiv({
    "P₁": 1,
    "P₂": 1,
    "P₃": 1,
    "P₄": 1,
    "Q":  1,
    "∞": -5
})
assert X1.eval(D) == (-1)^D.effective_degree() * D.eval(X1)

# We should get the same result here:
load("construct.sage")
points = [P1, P2, P3, P4, Q]
D = construct(points)
print(f"D  = {D}")

