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

