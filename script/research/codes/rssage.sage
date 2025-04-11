F = GF(11)
K.<x> = F[]
n, k = 10 , 5

C = codes.GeneralizedReedSolomonCode(F.list()[:n], k)
E = C.encoder("EvaluationPolynomial")

f = x^2 + 3*x + 10
c = E.encode(f)
print(c)

