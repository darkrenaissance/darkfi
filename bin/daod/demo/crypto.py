import random

def ff_inv(a, p):
    a %= p

    # extended euclidean algorithm
    # ps + at = 1
    t = 0
    new_t = 1
    r = p
    new_r = a

    while new_r != 0:
        quotient = r // new_r
        t, new_t = new_t, t - quotient * new_t
        r, new_r = new_r, r - quotient * new_r

    assert r == 1
    if t < 0:
        t += p

    return t

class EllipticCurve:

    def __init__(self, p, A, B, order, G, H):
        self.p = p
        self.A = A
        self.B = B
        self.order = order
        self.G = G
        self.H = H
        assert self.is_valid(G)
        assert self.is_valid(H)

    def is_valid(self, P):
        x, y, z = P
        if z == 0:
            return x != 0 or y != 0
        z_inv = ff_inv(z, self.p)
        x, y = x * z_inv, y * z_inv
        return y**2 % self.p == (x**3 + self.A * x + self.B) % self.p

    def add(self, p1, p2):
        x1, y1, z1 = p1
        x2, y2, z2 = p2

        if z1 == 0:
            return (x2, y2, z2)
        elif z2 == 0:
            return (x1, y1, z1)

        if x1 == x2:
            if y1 != y2:
                return (0, 1, 0)

            assert y1 != 0
            m = (3 * x1**2 + self.A) * ff_inv(2*y1, self.p)
        else:
            m = (y2 - y1) * ff_inv(x2 - x1, self.p)

        x3 = (m**2 - x1 - x2) % self.p
        y3 = (m * (x1 - x3) - y1) % self.p
        return (x3, y3, 1)

    def multiply(self, m, p):
        bits = f"{m:b}"
        result = (0, 1, 0)
        temp = p
        for bit in bits[::-1]:
            if bit == "1":
                result = self.add(result, temp)
            temp = self.add(temp, temp)
        return result

    def random_point(self):
        m = self.random_scalar()
        return self.multiply(m, self.G)

    def random_scalar(self):
        m = random.randrange(0, self.order - 1)
        return m

    def random_base(self):
        m = random.randrange(0, self.p - 1)
        return m

def pallas_curve():
    # Pallas
    p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
    q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
    G = (5, 5392431450607408583390510508521091931943415030464003135511088002453056875732, 1)
    H = (9762257241998025279988087154025308614062019274413483967640476725944341089207,
         12058632856930756995627167820351407063813260358041446014729496773111030695755, 1)
    ec = EllipticCurve(p, 0, 5, q, G, H)
    A = (144931808354919915876542440378319484704499556634959420306426167479163065488,
         2699682121356767698440748624399854659825391162912545787181017961871465868196, 1)
    B = (16017037670495191561606513965775243786961447026019262496667491008912834496943,
         20395164507282344548629891414360366999207473153143014512687861307997120664849, 1)
    assert ec.add(A, B) == (2414658659502531855741199170408914396997834981355655923471364687102714431309, 21133344194418979683767005688724798091220515434220043854575260979109407444719, 1)
    m = 26322809409216846271933211244226061368157231119725763192402071651286829040466
    assert ec.multiply(m, G) == (15862887453366837597569434439063150886012590021428640083047997467990450633825, 25887284719793568129480941070850220101898092026705204234126448799557008384178, 1)
    return ec

