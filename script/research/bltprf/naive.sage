q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
K = GF(q)
a = K(0x00)
b = K(0x05)
E = EllipticCurve(K, (a, b))
G = E(0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000000, 0x02)

p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
assert E.order() == p
Scalar = GF(p)

a1, a2, a3, a4, a5, a6, a7, a8, a9, a10 = (
    Scalar(110), Scalar(56), Scalar(89), Scalar(6543), Scalar(2),
    Scalar(110), Scalar(44), Scalar(78), Scalar(77), Scalar(4))

G1, G2, G3, G4, G5, G6, G7, G8, G9, G10 = (
    E.random_element(), E.random_element(), E.random_element(),
    E.random_element(), E.random_element(), E.random_element(),
    E.random_element(), E.random_element(), E.random_element(),
    E.random_element())

A = (int(a1) * G1 + int(a2) * G2 + int(a3) * G3 + int(a4) * G4
     + int(a5) * G5 + int(a6) * G6 + int(a7) * G7 + int(a8) * G8
     + int(a9) * G9 + int(a10) * G10)

a1, a2, a3, a4, a5 = (a1, a2), (a3, a4), (a5, a6), (a7, a8), (a9, a10)
G1, G2, G3, G4, G5 = (G1, G2), (G3, G4), (G5, G6), (G7, G8), (G9, G10)

# a1 G1-\  a2 G1    a3 G1    a4 G1    a5 G1
# a1 G2  \-a2 G2-\  a3 G2    a4 G2    a5 G2
# a1 G3    a2 G3  \-a3 G3-\  a4 G3    a5 G3
# a1 G4    a2 G4    a3 G4  \-a4 G4-\  a5 G4
# a1 G5    a2 G5    a3 G5    a4 G5  \-a5 G5

# Dot product
def dot(x, y):
    result = None
    for x_i, y_i in zip(x, y):
        if result is None:
            result = int(x_i) * y_i
        else:
            result += int(x_i) * y_i
    return result

# Main diagonal is sum(a_i G_i) = A
assert dot(a1, G1) + dot(a2, G2) + dot(a3, G3) + dot(a4, G4) + dot(a5, G5) == A

# Sum all the diagonals of the grid above
A_neg_4 = dot(a1, G5)
A_neg_3 = dot(a1, G4) + dot(a2, G5)
A_neg_2 = dot(a1, G3) + dot(a2, G4) + dot(a3, G5)
A_neg_1 = dot(a1, G2) + dot(a2, G3) + dot(a3, G4) + dot(a4, G5)
A_0 = A
A_1 = dot(a2, G1) + dot(a3, G2) + dot(a4, G3) + dot(a5, G4)
A_2 = dot(a3, G1) + dot(a4, G2) + dot(a5, G3)
A_3 = dot(a4, G1) + dot(a5, G2)
A_4 = dot(a5, G1)

x = Scalar.random_element()
a_prime = (x * vector(a1) + x^2 * vector(a2)
           + x^3 * vector(a3) + x^4 * vector(a4)
           + x^5 * vector(a5))
# Sage cannot do this:
#
# G_prime = (int(x^-1) * vector(G1) + int(x^-2) * vector(G2)
#            + int(x^-3) * vector(G3) + int(x^-4) * vector(G4)
#            + int(x^-5) * vector(G5))
G_prime = [(int(x^-1) * G1[0] + int(x^-2) * G2[0] + int(x^-3) * G3[0]
            + int(x^-4) * G4[0] + int(x^-5) * G5[0]),
           (int(x^-1) * G1[1] + int(x^-2) * G2[1] + int(x^-3) * G3[1]
            + int(x^-4) * G4[1] + int(x^-5) * G5[1])]
assert len(a_prime) == len(G_prime) == 2
A_prime = dot(a_prime, G_prime)

assert (int(x^-4) * A_neg_4 + int(x^-3) * A_neg_3 + int(x^-2) * A_neg_2
        + int(x^-1) * A_neg_1
        + A
        + int(x) * A_1 + int(x^2) * A_2 + int(x^3) * A_3 + int(x^4) * A_4) \
       == A_prime

