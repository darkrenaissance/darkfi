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
P5 = LabelPoint(E.random_element(), {"P₅": 1})
Q = -(P1.P + P2.P + P3.P + P4.P + P5.P)
Q = LabelPoint(Q, {"Q": 1})
assert P1.P + P2.P + P3.P + P4.P + P5.P + Q.P == E(0)

L1 = div_line(P1, P2)
Q1 = P1 + P2
print(f"L₁ = {L1}")
print(f"Q₁ = P₁ + P₂")
L2 = div_line(P3, P4)
Q2 = P3 + P4
print(f"L₂ = {L2}")
print(f"Q₂ = P₃ + P₄")
L3 = div_line(P5, Q)
Q3 = P5 + Q
print(f"L₃ = {L3}")
print(f"Q₃ = P₅ + Q")
print()

ℓ4 = div_line(Q1, Q2)
L4 = ℓ4 + L1 + L2 - div_line(Q1, -Q1) - div_line(Q2, -Q2)
Q4 = Q1 + Q2
print(f"ℓ₄ = {ℓ4}")
print(f"L₄ = ℓ₄ + L₁ + L₂ - div(x - Q₁) - div(x - Q₂)")
print(f"   = {L4}")
print(f"Q₄ = Q₁ + Q₂")
print("Carry L₃ to next level")
print()

ℓ5 = div_line(Q4, Q3)
L5 = ℓ5 + L4 + L3 - div_line(Q4, -Q4) - div_line(Q3, -Q3)
print(f"ℓ₅ = {ℓ5}")
print(f"L₅ = ℓ₅ + L₄ + L₃ - div(x - Q₄) - div(x - Q₃)")
print(f"   = {L5}")
print()

# We should get the same result here:
load("construct.sage")
points = [P1, P2, P3, P4, P5, Q]
L = construct(points)
print(f"L  = {L}")

