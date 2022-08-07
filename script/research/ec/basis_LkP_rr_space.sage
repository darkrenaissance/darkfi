# See https://math.stackexchange.com/questions/294644/basis-for-the-riemann-roch-space-lkp-on-a-curve?rq=1
# Basis for the Riemann-Roch space L(kP) on a curve
# find a basis for L(k[P])

# Compute basis elements for L(n[P]) on y^2 = x^3 - x at P = (0, 0)

R.<x> = FunctionField(QQbar)
S.<Y> = R[]
L.<y> = R.extension(Y^2 - (x^3 - x))

# Verify that P is ordinary with the ideal <x - 0, y - 0>
I = L.maximal_order().ideal(x,y)
assert I.is_prime()

D = I.divisor()
print("L([P]) =", D.basis_function_space())
print("L(2[P]) =", (2*D).basis_function_space())
print("L(3[P]) =", (3*D).basis_function_space())
print("L(4[P]) =", (4*D).basis_function_space())
print("L(5[P]) =", (5*D).basis_function_space())
print("L(6[P]) =", (6*D).basis_function_space())
print("L(7[P]) =", (7*D).basis_function_space())

