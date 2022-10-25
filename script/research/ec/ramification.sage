K.<x> = FunctionField(GF(11)); _.<Y> = K[]
L.<y> = K.extension(Y^2 - x^3 - 4*x)
# P = (2, 4)
p = L.places_finite()[-3]
R = p.valuation_ring()
print((y - 2*x).valuation(p))

