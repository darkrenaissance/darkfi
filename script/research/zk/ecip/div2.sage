class Divisor:

    def __init__(self, func, support):
        self.func = func
        self.support = support

    def __repr__(self):
        rep = ""
        first = True
        for v, P in self.support:
            if first:
                if v < 0:
                    rep = "- "
                first = False
            else:
                if v < 0:
                    rep += " - "
                else:
                    rep += " + "
            v = abs(v)
            if v == 1:
                rep += f"[{P}]"
            else:
                rep += f"{v}[{P}]"
        return rep

    def rename(self, symbol, new_point):
        for i, (v, P) in enumerate(self.support):
            if repr(P) == symbol:
                self.support[i][1] = new_point

    def copy_support(self):
        support = []
        for v, P in self.support:
            support.append([v, P.copy()])
        return support

    def __add__(self, other):
        support = self.copy_support()
        for n, P in other.support:
            found = False
            for i, (_, Q) in enumerate(support):
                if P.P == Q.P:
                    found = True
                    support[i][0] += n
            if not found:
                support.append([n, P])
        func = self.func * other.func
        return Divisor(func, support)._cleanup()

    def _cleanup(self):
        # Cleanup
        support = []
        for n, P in self.support:
            if n == 0:
                continue
            support.append([n, P])
        self.support = support
        return self

    def __neg__(self):
        support = []
        for n, P in self.support:
            support.append([-n, P])
        func = 1/self.func
        return Divisor(func, support)

    def __sub__(self, other):
        other = -other
        return self + other

    def is_equiv(self, support):
        support = support.copy()
        for n, P in self.support:
            P = repr(P)
            if P not in support:
                return False
            if n != support[P]:
                return False
            del support[P]
        return not support

    def eval(self, other):
        f = self.func
        result = 1
        for n, P in other.support:
            if P.P == E(0, 1, 0):
                continue
            Px, Py = P.P.xy()
            result *= f(x=Px, y=Py)^n
        return result

    def effective_degree(self):
        deg = 0
        for n, P in self.support:
            if P.P == E(0, 1, 0):
                continue
            deg += n
        return deg

def slope_intercept(P1, P2):
    P1x, P1y = P1.xy()
    P2x, P2y = P2.xy()
    P3x, P3y = (-(P1 + P2)).xy()
    λ = (P2y - P1y) / (P2x - P1x)
    μ = P2y - λ*P2x
    return λ, μ

def line(P1, P2):
    if -(P1 + P2) == E(0, 1, 0):
        assert P1[0] == P2[0]
        return x - P1[0]

    if P1 == P2:
        # Use P3 instead
        P2 = -(P1 + P2)

    λ, μ = slope_intercept(P1, P2)
    return y - λ*x - μ

class LabelPoint:

    def __init__(self, P, labels):
        self.P = P
        self.labels = labels

    def _add_labels(self, other_labels):
        labels = self.labels.copy()
        for key, value in other_labels.items():
            if key in labels:
                labels[key] += value
            else:
                labels[key] = value
        return labels

    def copy(self):
        return LabelPoint(self.P, self.labels.copy())

    def __add__(self, Q):
        P = self.P + Q.P
        labels = self._add_labels(Q.labels)
        return LabelPoint(P, labels)

    def __mul__(self, n):
        labels = self.labels.copy()
        for key in labels:
            labels[key] *= n
        return LabelPoint(n*self.P, labels)

    def __neg__(self):
        P = -self.P
        labels = self.labels.copy()
        for key in labels:
            labels[key] *= -1
        return LabelPoint(P, labels)

    def __repr__(self):
        rep = ""
        first = True
        for P, m in self.labels.items():
            if first:
                if m < 0:
                    rep = "-"
                first = False
            else:
                if m < 0:
                    rep += " - "
                else:
                    rep += " + "
            m = abs(m)
            if m == 1:
                rep += f"{P}"
            else:
                rep += f"{m}{P}"
        return rep

def div_line(P1, P2):
    inf = LabelPoint(E(0, 1, 0), {"∞": 1})
    P3 = -(P1 + P2)
    if P1 == P2:
        support = [
            [ 2, P1],
            [ 1, P3],
            [-3, inf]
        ]
    elif P3.P == E(0, 1, 0):
        support = [
            [ 1, P1],
            [ 1, P2],
            [-2, inf]
        ]
    else:
        support = [
            [ 1, P1],
            [ 1, P2],
            [ 1, P3],
            [-3, inf]
        ]
    func = line(P1.P, P2.P)
    return Divisor(func, support)

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

D1 = div_line(P2, P2)
assert X1.eval(D1) == (-1)^D1.effective_degree() * D1.eval(X1)
D2 = div_line(P1, P1)
assert X1.eval(D2) == (-1)^D2.effective_degree() * D2.eval(X1)
D3 = div_line(P2, -P2)
assert X1.eval(D3) == (-1)^D3.effective_degree() * D3.eval(X1)
D4 = div_line(P0, -P0)
assert X1.eval(D4) == (-1)^D4.effective_degree() * D4.eval(X1)
D5 = D1 + D2 + D3 + D4
assert D5.is_equiv({
    "P₀":   1,
    "P₁":   2,
    "P₂":   3,
    "-P₀":  1,
    "-2P₁": 1,
    "-2P₂": 1,
    "-P₂":  1,
    "∞":  -10
})
assert X1.eval(D5) == (-1)^D5.effective_degree() * D5.eval(X1)

D6 = div_line(P2*-2, P1*-2)
assert X1.eval(D6) == (-1)^D6.effective_degree() * D6.eval(X1)
D7 = div_line(-P2, -P0)
assert X1.eval(D7) == (-1)^D7.effective_degree() * D7.eval(X1)
D8 = D5 - (D6 + D7)
assert D8.is_equiv({
    "P₀":         1,
    "P₁":         2,
    "P₂":         3,
    "2P₂ + 2P₁": -1,
    "P₂ + P₀":   -1,
    "∞":         -4
})
assert X1.eval(D8) == (-1)^D8.effective_degree() * D8.eval(X1)

D9 = div_line(P0 + P2, (P1 + P2)*2)
D10 = D8 + D9
assert D10.is_equiv({
    "P₀": 1,
    "P₁": 2,
    "P₂": 3,
    "-P₀ - 3P₂ - 2P₁": 1,
    "∞": -7
})
D10.rename("-P₀ - 3P₂ - 2P₁", Q*5)
assert X1.eval(D10) == (-1)^D10.effective_degree() * D10.eval(X1)

D11 = div_line(Q, Q)
D12 = div_line(-Q*2, Q*2)
D13 = D11 - D12
assert D13.is_equiv({
    "Q": 2,
    "2Q": -1,
    "∞": -1
})
assert X1.eval(D13) == (-1)^D13.effective_degree() * D13.eval(X1)

D14 = D13 + D13 + D13
assert D14.is_equiv({
    "Q": 6,
    "2Q": -3,
    "∞": -3
})
D15 = div_line(Q*2, Q*2)
D16 = D14 + D15
assert D16.is_equiv({
    "Q": 6,
    "2Q": -1,
    "-4Q": 1,
    "∞": -6
})

D17 = div_line(Q*2, -Q*2)
D18 = D16 + D17
assert D18.is_equiv({
    "Q": 6,
    "-2Q": 1,
    "-4Q": 1,
    "∞": -8
})

D19 = div_line(-Q*2, -Q*4)
D20 = D18 - D19
assert D20.is_equiv({
    "Q": 6,
    "6Q": -1,
    "∞": -5
})

D21 = div_line(Q*6, -Q*6)
D22 = D20 + D21
D23 = div_line(Q, -Q*6)
D24 = D22 - D23
D = D10 + D24
assert D.is_equiv({
    "P₀": 1,
    "P₁": 2,
    "P₂": 3,
    "Q":  5,
    "∞": -11
})
assert X1.eval(D) == (-1)^D.effective_degree() * D.eval(X1)

