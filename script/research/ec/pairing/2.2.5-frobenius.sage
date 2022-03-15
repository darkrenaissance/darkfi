q = 67
K.<x> = GF(q)[]
F1.<u> = K.extension(x^2 + 1)
F2.<v> = F1.extension(x^3 + 2)

E = EllipticCurve(F2, [4, 3])

print(E)
pi_i = F2.frobenius_endomorphism()
#print(pi)

def pi(p):
    return E(pi_i(p[0]), pi_i(p[1]))

P1 = E(15, 50)
print(pi(P1))

P2 = E(2*u + 16, 30*u + 39)
print(pi(P2))

P3 = E(15*v^2 + 4*v + 8, 44*v^2 + 30*v + 21)
print(pi(P3))
print(pi(pi(P3)))
print(pi(pi(pi(P3))))

