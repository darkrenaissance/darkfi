DIV_POINT = 1
DIV_FUNC  = 2

class Divisor:

    def __init__(self, field):
        self.field = field
        self._div = []

    def __call__(self, Px, Py, Pz=1):
        K = self.field
        # Convert to base field
        Px, Py, Pz = K(Px), K(Py), K(Pz)
        # Normalize coordinates
        if Pz > 0:
            Px /= Pz
            Py /= Pz
            Pz = 1
        P = (Px, Py, Pz)

        D = Divisor(K)
        D._div += [(DIV_POINT, 1, P)]
        return D

    def div(self, f):
        K = self.field
        D = Divisor(K)
        D._div += [(DIV_FUNC, 1, f)]
        return D

    def _clean(self):
        self._div = [(type_id, self._deg_obj(P), P)
                     for type_id, P in self._objs()]
        self._div = [(type_id, n, P) for type_id, n, P in self._div if n != 0]
    def _deg_obj(self, P):
        return sum(n for type_id, n, Q in self._div if P == Q)
    def _objs(self):
        return set((type_id, P) for type_id, _, P in self._div)

    def __add__(self, other):
        K = self.field
        D = Divisor(K)
        D._div = self._div[:] + other._div[:]
        D._clean()
        return D

    def __sub__(self, other):
        K = self.field
        D = self + -1*other
        return D

    def __mul__(self, n):
        K = self.field
        D = Divisor(K)
        D._div = [(type_id, n*m, P) for type_id, m, P in self._div]
        return D

    __rmul__ = __mul__

    def __repr__(self):
        out = ""
        if not self._div:
            out += "0"
        for i, (type_id, n, obj) in enumerate(self._div):
            assert n != 0
            if i > 0:
                if n > 0:
                    out += " + "
                else:
                    out += " - "
            else:
                if n < 0:
                    out += "-"
            assert type_id in (DIV_POINT, DIV_FUNC)
            if abs(n) > 1:
                out += f"{abs(n)}"
            if type_id == DIV_POINT:
                out += self._format_point(obj)
            elif type_id == DIV_FUNC:
                out += f" div({obj})"
        return out

    def _format_point(self, P):
        Px, Py, Pz = P
        assert Pz in (0, 1)
        if Pz == 0:
            return f"[∞]"
        return f"[({Px}, {Py})]"

    def deg(self):
        return sum(n for _, n, _ in self._div)

    def supp(self):
        return set(P for _, _, P in self._div)

K.<x, y> = GF(47)[]
D = Divisor(K)
D = 4*D(2, 3) + D(2, 4) + 2*D(0, 1, 0) + 4*D.div(x^2 + y)
E = 6*D(4, 2) - 6*D(6, 3) + 6*D(6, 3)
#D -= 4*D.div(x^2 + y)
#E -= 6*D(4, 2)
print(f"D = {D}")
print(f"E = {E}")
print(f"D + E = {D + E}")
print(f"6E = {6*E}")
print(f"deg(D) = {D.deg()}")
print(f"supp(D) = {D.supp()}")
print(f"deg(E) = {E.deg()}")
print(f"supp(E) = {E.supp()}")

