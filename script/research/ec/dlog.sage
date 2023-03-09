# Initialize an elliptic curve
p = 115792089237316195423570985008687907853269984665640564039457584007908834671663
r = 115792089237316195423570985008687907852837564279074904382605163141518161494337
Fp = GF(p)  # Base Field
Fr = GF(r)  # Scalar Field
A = 0
B = 7
E = EllipticCurve(GF(p), [A,B])
assert(E.cardinality() == r)

K.<x> = PolynomialRing(Fp, implementation="generic")
L.<y> = PolynomialRing(K, implementation="generic")
M.<z> = L[]
eqn = y^2 - x^3 - A * x - B

# Returns line passing through points, works for all points and returns 1 for O + O = O
def line(A, B):
    if A == 0 and B == 0:
        return 1
    else:
        [a, b, c] = Matrix([A, B, -(A+B)]).transpose().kernel().basis()[0]
        return a*x + b*y + c

def dlog(D):
    # Derivative via partials
    Dx = D.differentiate(x)
    Dy = D.differentiate(y)
    Dz = Dx + Dy * ((3*x^2 + A) / (2*y))
    assert D != 0
    return Dz/D

def dlog_alt(D):
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
    Dz_numer = (2*y*Dx + Dy * (3*x^2 + A) * V(y=-y)).mod(eqn)
    # Change denominator to the norm
    D_denom = (V * V(y=-y)).mod(eqn)

    # This calculation is quite slow...
    print("Calculating Dlog = Dz/D")
    return Dz_numer / D_denom

# Accepts arbitrary list of points, including duplicates and inverses, and constructs function
# intersecting exactly those points if they form a principal divisor (i.e. sum to zero).
def construct_function(Ps):
    # List of intermediate sums/principal divisors, removes 0
    xs = [(P, line(P, -P)) for P in Ps if P != 0]

    while len(xs) != 1:
        assert(sum(P for (P, _) in xs) == 0)
        xs2 = []

        # Carry extra point forward
        if mod(len(xs), 2) == 1:
            x0 = xs[0]
            xs = xs[1:]
        else:
            x0 = None

        # Combine the functions for all pairs
        for n in range(0, floor(len(xs)/2)):
            (A, aNum) = xs[2*n]
            (B, bNum) = xs[2*n+1]

            # Divide out intermediate (P, -P) factors
            num = L((aNum * bNum * line(A, B)).mod(eqn))
            den = line(A, -A) * line(B, -B)
            D = num / K(den)
            
            # Add new element
            xs2.append((A+B, D))
        
        if x0 != None:
            xs2.append(x0)

        xs = xs2
    
    assert(xs[0][0] == 0)
    
    # Normalize, might fail but negl probability for random points. Must be done for zkps
    # although free to use any coefficient
    return D / D(x=0, y=0)

P0 = E.random_element()
P1 = E.random_element()
P2 = E.random_element()
Q = -int(Fr(5)^-1) * (P0 + 2*P1 + 3*P2)
assert P0 + 2*P1 + 3*P2 + 5*Q == 0

def div_add(div_f, div_g):
    div = div_f.copy()
    for P, n in div_g.items():
        if P in div:
            div[P] += n
        else:
            div[P] = n
    div = dict((P, n) for P, n in div.items() if n != 0)
    return div

def div_invert(div):
    return dict((P, -n) for P, n in div.items())

def div_sub(div_f, div_g):
    inv_div_g = div_invert(div_g)
    return div_add(div_f, inv_div_g)

# 2[P₂] + [-2P₂] - 3[∞]
f1 = line(P2, P2)
D1 = {"P2": 2, "-2P2": 1, "∞": -3}
# 2[P₁] + [-2P₁] - 3[∞]
f2 = line(P1, P1)
D2 = {"P1": 2, "-2P1": 1, "∞": -3}
# [P₂] + [-P₂] - 2[∞]
f3 = line(P2, -P2)
D3 = {"P2": 1, "-P2": 1, "∞": -2}
# [P₀] + [-P₀] - 2[∞]
f4 = line(P0, -P0)
D4 = {"P0": 1, "-P0": 1, "∞": -2}

# (2[P₂] + [-2P₂] - 3[∞]
#  + 2[P₁] + [-2P₁] - 3[∞]
#  + [P₂] + [-P₂] - 2[∞]
#  + [P₀] + [-P₀] - 2[∞])
# =
# [P₀] + 2[P₁] + 3[P₂] + [-P₀] + [-2P₁] + [-2P₂] + [-P₂] - 10[∞]
f5 = f1*f2*f3*f4
D5 = div_add(div_add(D1, D2), div_add(D3, D4))
assert D5 == {
    "P0":   1,
    "P1":   2,
    "P2":   3,
    "-P0":  1,
    "-2P1": 1,
    "-2P2": 1,
    "-P2":  1,
    "∞":  -10
}

# [-2P₂] + [-2P₁] + [2(P₁ + P₂)] - 3[∞]
f6 = line(-2*P2, -2*P1)
D6 = {"-2P2": 1, "-2P1": 1, "2P1 + 2P2": 1, "∞": -3}
# [-P₂] + [-P₀] + [P₀ + P₂] - 3[∞]
f7 = line(-P2, -P0)
D7 = {"-P2": 1, "-P0": 1, "P0 + P2": 1, "∞": -3}

# ([P₀] + 2[P₁] + 3[P₂] + [-P₀] + [-2P₁] + [-2P₂] + [-P₂] - 10[∞]
#  - [-2P₂] - [-2P₁] - [2(P₁ + P₂)] + 3[∞]
#  - [-P₂] - [-P₀] - [P₀ + P₂] + 3[∞])
# =
# [P₀] + 2[P₁] + 3[P₂] - [2(P₁ + P₂)] - [P₀ + P₂] - 4[∞]
f8 = f5/(f6*f7)
D8 = div_sub(D5, div_add(D6, D7))
assert D8 == {
    "P0":         1,
    "P1":         2,
    "P2":         3,
    "2P1 + 2P2": -1,
    "P0 + P2":   -1,
    "∞":         -4
}

# [P₀ + P₂] + [2(P₁ + P₂)] + [-(P₀ + 2P₁ + 3P₂)] - 3[∞]
f9 = line(P0 + P2, 2*(P1 + P2))
D9 = {"P0 + P2": 1, "2P1 + 2P2": 1, "5Q": 1, "∞": -3}
# ([P₀] + 2[P₁] + 3[P₂] - [2(P₁ + P₂)] - [P₀ + P₂] - 4[∞]
#  + [P₀ + P₂] + [2(P₁ + P₂)] + [-(P₀ + 2P₁ + 3P₂)] - 3[∞])
# =
# [P₀] + 2[P₁] + 3[P₂] + [-(P₀ + 2P₁ + 3P₂)] - 7[∞]
# = [P₀] + 2[P₁] + 3[P₂] + [5Q] - 7[∞]
f10 = f8*f9
D10 = div_add(D8, D9)
assert D10 == {
    "P0": 1,
    "P1": 2,
    "P2": 3,
    "5Q": 1,
    "∞": -7
}

# Now construct 5[Q]

# 2[Q] + [-2Q] - 3[∞]
f11 = line(Q, Q)
D11 = {"Q": 2, "-2Q": 1, "∞": -3}

# [-2Q] + [2Q] - 2[∞]
f12 = line(-2*Q, 2*Q)
D12 = {"-2Q": 1, "2Q": 1, "∞": -2}

# (2[Q] + [-2Q] - 3[∞]) - ([-2Q] + [2Q] - 2[∞])
# ==
# 2[Q] - [2Q] - [∞]
f13 = f11/f12
D13 = div_sub(D11, D12)
assert D13 == {
    "Q": 2,
    "2Q": -1,
    "∞": -1
}

# multiply by 3
# 6[Q] - 3[2Q] - 3[∞]
f14 = f13*f13*f13
D14 = div_add(div_add(D13, D13), D13)
assert D14 == {
    "Q": 6,
    "2Q": -3,
    "∞": -3
}

# 2[2Q] + [-4Q] - 3[∞]
f15 = line(2*Q, 2*Q)
D15 = {"2Q": 2, "-4Q": 1, "∞": -3}

# (6[Q] - 3[2Q] - 3[∞]) + (2[2Q] + [-4Q] - 3[∞])
# ==
# 6[Q] - [2Q] + [-4Q] - 6[∞]
f16 = f14*f15
D16 = div_add(D14, D15)
assert D16 == {
    "Q": 6,
    "2Q": -1,
    "-4Q": 1,
    "∞": -6
}

# [2Q] + [-2Q] - 2[∞]
f17 = line(2*Q, -2*Q)
D17 = {"2Q": 1, "-2Q": 1, "∞": -2}

# (6[Q] - [2Q] + [-4Q] - 6[∞]) + ([2Q] + [-2Q] - 2[∞])
# ==
# 6[Q] + [-2Q] + [-4Q] - 8[∞]
f18 = f16*f17
D18 = div_add(D16, D17)
assert D18 == {
    "Q": 6,
    "-2Q": 1,
    "-4Q": 1,
    "∞": -8
}

# [-2Q] + [-4Q] + [6Q] - 3[∞]
f19 = line(-2*Q, -4*Q)
D19 = {"-2Q": 1, "-4Q": 1, "6Q": 1, "∞": -3}

# (6[Q] + [-2Q] + [-4Q] - 8[∞]) - ([-2Q] + [-4Q] + [6Q] - 3[∞])
# ==
# 6[Q] - [6Q] - 5[∞]
f20 = f18/f19
D20 = div_sub(D18, D19)
assert D20 == {
    "Q": 6,
    "6Q": -1,
    "∞": -5
}

# [6Q] + [-6Q] - 2[∞]
f21 = line(6*Q, -6*Q)
D21 = {"6Q": 1, "-6Q": 1, "∞": -2}

# (6[Q] - [6Q] - 5[∞]) + ([6Q] + [-6Q] - 2[∞])
# ==
# 6[Q] + [-6Q] - 7[∞]
f22 = f20*f21
D22 = div_add(D20, D21)
assert D22 == {"Q": 6, "-6Q": 1, "∞": -7}

# [Q] + [-6Q] + [5Q] - 3[∞]
f23 = line(Q, -6*Q)
D23 = {"Q": 1, "-6Q": 1, "5Q": 1, "∞": -3}

# (6[Q] + [-6Q] - 7[∞]) - ([Q] + [-6Q] + [5Q] - 3[∞])
# ==
# 5[Q] - [5Q] - 4[∞]
f24 = f22/f23
D24 = div_sub(D22, D23)
assert D24 == {"Q": 5, "5Q": -1, "∞": -4}

# Now combine the result
f = f10*f24
D = div_add(D10, D24)
assert D == {
    "P0": 1,
    "P1": 2,
    "P2": 3,
    "Q":  5,
    "∞": -11
}

f_numer = f.numerator().mod(eqn)
f_denom = f.denominator().mod(eqn)
# ZeroDivisionError
#DLog = dlog(f_numer)

assert f(x=P0[0], y=P0[1]) == 0
assert f(x=P1[0], y=P1[1]) == 0
assert f(x=P2[0], y=P2[1]) == 0
# Need to modify f because this is 0/0
#assert f(x=Q[0], y=Q[1]) == 0

f_denom *= f_denom(y=-y)
f_denom = K(f_denom.mod(eqn))
f_numer *= f_denom(y=-y)
f_numer = f_numer.mod(eqn)
f = f_numer / f_denom
print("Created f such that div(f) = D")

#Ps = [P0] + 2*[P1] + 3*[P2] + 5*[Q]
#D = construct_function(Ps)
#
#assert D(x=P0[0], y=P0[1]) == 0
#assert D(x=P1[0], y=P1[1]) == 0
#assert D(x=P2[0], y=P2[1]) == 0
#assert D(x=Q[0], y=Q[1]) == 0

# This will fail due to a bug in sage:
#DLog = dlog(D)
# ZeroDivisionError
#DLog = dlog_alt(f)

print("Random A₀, A₁")
[A0, A1] = [E.random_element() for _ in range(2)]
A2 = -(A0 + A1)
A0x, A0y = A0.xy()
A1x, A1y = A1.xy()
A2x, A2y = A2.xy()
λ = (A1y - A0y) / (A1x - A0x)
μ = A1y - λ*A1x
assert A2y - λ*A2x - μ == 0

f_a = K(f(y=0))
f_b = K((f - f_a)(y=1))
assert f_a + y*f_b == f
#deg_f = f_b.degree() + 1

f_A = 1
for Ai in [A0, A1, A2]:
    Aix, Aiy = Ai.xy()
    f_A *= f(x=Aix, y=Aiy)
print(f_A)

g = y - λ*x - μ
g_P = 1
for Pi, v in [(P0, 1), (P1, 2), (P2, 3), (Q, 5)]:
    Pix, Piy = Pi.xy()
    g_P *= -g(x=Pix, y=Piy)^v
print(g_P)

