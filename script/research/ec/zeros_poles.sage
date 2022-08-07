K.<x, y> = GF(11)[]
EC_A, EC_B = 4, 0
C = y^2 - x^3 - EC_A*x - EC_B
S = K.quotient(C).fraction_field()
X, Y = S(x), S(y)

foo = (X^2 - Y) / (X - 2)
f, g = foo.numerator().lift(), foo.denominator().lift()

I = ideal(C, f)
J = ideal(C, g)

load("ordp.sage")

total_k = 0
for info in I.variety():
    Px, Py = P = info[x], info[y]
    k = ordp(P, f)
    print(f"ordp(({Px}, {Py}), {f}) = {k}")
    total_k += k
for info in J.variety():
    Px, Py = P = info[x], info[y]
    k = ordp(P, g)
    print(f"ordp(({Px}, {Py}), 1/({g})) = -{k}")
    total_k -= k
print(f"ordp(∞, ({f})/({g}) = -{total_k}")

