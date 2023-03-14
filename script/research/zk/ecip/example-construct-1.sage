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

P0 = LabelPoint(E.random_element(), {"P₀": 1})
P1 = LabelPoint(E.random_element(), {"P₁": 1})
P2 = LabelPoint(E.random_element(), {"P₂": 1})
Q = -int(Fr(5)^-1) * (P0.P + 2*P1.P + 3*P2.P)
Q = LabelPoint(Q, {"Q": 1})

A0 = LabelPoint(E.random_element(), {"A₀": 1})
A1 = LabelPoint(E.random_element(), {"A₁": 1})
X1 = div_line(A0, A1)

# i = 0

L0 = div_line(P0, -P0)
L1 = div_line(P1, -P1)
L2 = div_line(P2, -P2)
L3 = div_line(Q, -Q)

# P₀ + 2P₁ + 3P₂ + 5Q

# i = 1
D1 = div_line(P1, P1)
R1 = P1 + P1

D2 = div_line(P2, P2)
R2 = P2 + P2

D3 = div_line(P2, Q)
R3 = P2 + Q

D4 = div_line(Q, Q)
R4 = Q + Q

D5 = D4
R5 = R4

D6 = L0
R6 = P0

# i = 2

D1 = D1 + D2 + div_line(R1, R2) - (div_line(R1, -R1) + div_line(R2, -R2))
R1 = R1 + R2

D2 = D3 + D4 + div_line(R3, R4) - (div_line(R3, -R3) + div_line(R4, -R4))
R2 = R3 + R4

D3 = D5 + D6 + div_line(R5, R6) - (div_line(R5, -R5) + div_line(R6, -R6))
R3 = R5 + R6

# i = 3

Dx = D1
Rx = R1

D1 = D2 + D3 + div_line(R2, R3) - (div_line(R2, -R2) + div_line(R3, -R3))
R1 = R2 + R3

D2, R2 = Dx, Rx

# i = 4

D = D1 + D2 + div_line(R1, R2) - (div_line(R1, -R1) + div_line(R2, -R2))
assert D.is_equiv({
    "P₀": 1,
    "P₁": 2,
    "P₂": 3,
    "Q":  5,
    "∞": -11
})
assert X1.eval(D) == (-1)^D.effective_degree() * D.eval(X1)

f_numer = D.func.numerator().mod(eqn)
f_denom = D.func.denominator().mod(eqn)
f = f_numer / f_denom
assert f.denominator() == 1
f = f.numerator()
print(f)
save([P0.P, P1.P, P2.P, Q.P, f], "div.sobj")
print("Saved (P₀, P₁, P₂, Q, f)")

