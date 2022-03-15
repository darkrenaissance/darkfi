q = 19
Fq = GF(q)
E = EllipticCurve(Fq, [0, 5])
assert q % 3 == 1

xi3 = Fq(7)
assert xi3^3 == 1

P = E(-1, 2)
print(f"xi(P) = {E(xi3 * -1, 2)}")

Fq.<x> = GF(23)[]
Fq2.<u> = Fq.extension(x^2 + 1)
E = EllipticCurve(Fq2, [0, 5])
P = E(-1, 2)

xi3 = 8 * u + 11
print(f"xi(P) = {E(-xi3, 2)}")

