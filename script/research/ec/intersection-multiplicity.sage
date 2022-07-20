# Calculate intersection multiplicity of a point in sage
q = 47
K = GF(q)
E = EllipticCurve(K, (0, 5))
Q = E(10, 26)

C = E.defining_polynomial()

R.<x, y, z> = PolynomialRing(K)
f = y - Q[1] * z

P.<x,y,z> = ProjectiveSpace(K, 2)
X = P.subscheme([C(x, y, z)])
Y = P.subscheme([f(x, y, z)])

Q = X([Q[0], Q[1]])
print(Q.intersection_multiplicity(Y))

