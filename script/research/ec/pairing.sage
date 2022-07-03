# see
# https://github.com/zcash/librustzcash/blob/6e0364cd42a2b3d2b958a54771ef51a8db79dd29/pairing/src/bls12_381/README.md#generators

xxx = -0xd201000000010000
q = 0x1a0111ea397fe69a4b1ba7b6434bacd764774b84f38512bf6730d2a0f6b0f6241eabfffeb153ffffb9feffffffffaaab
assert q == (xxx - 1)^2 * ((xxx^4 - xxx^2 + 1) / 3) + xxx
# 381 bits to represent q
assert 380 < log(q, 2).n() < 381
r = 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001
assert r == (xxx^4 - xxx^2 + 1)
assert 254 < log(r, 2).n() < 255


# F₂ is constructed as F(u) / (u² + 1)
# F₆ is constructed as F₂(v) / (v³- (u + 1))
# F₁₂ is constructed as F₆(w) / (w²- v)

# we can't do extension field towers in sage...
# https://ask.sagemath.org/question/49663/efficiently-computing-tower-fields-for-pairings/
F1 = GF(q)
K2.<x> = PolynomialRing(F1)
F2.<u> = F1.extension(x^2 + 1)

# See file bls-init.sage for an explanation
R.<y> = PolynomialRing(F2)
K.<w> = F2.extension(y^6 - (u + 1))
v = w^2
print(f"u = {u}")
print(f"v = {v}")
print(f"w = {w}")

assert u^2 + 1 == 0
assert v^3 - (u + 1) == 0
assert w^2 - v == 0

E1 = EllipticCurve(K, (0, 4))
E2 = EllipticCurve(K, (0, 4*(u + 1)))

def find_random_point(E, F, A, B):
    while True:
        x = F.random_element()
        y = sqrt(x^3 + A*x + B)
        return E(x, y)

E1.random_point = lambda: find_random_point(E1, F1, 0, 4)
E2.random_point = lambda: find_random_point(E2, F2, 0, 4*(u + 1))

x1 = 3685416753713387016781088315183077757961620795782546409894578378688607592378376318836054947676345821548104185464507
y1 = 1339506544944476473020471379941921221584933875938349620426543736416511423956333506472724655353366534992391756441569
G1 = E1(x1, y1)
#assert G1.order() == r

x2 = 3059144344244213709971259814753781636986470325476647558659373206291635324768958432433509563104347017837885763365758*u + 352701069587466618187139116011060144890029952792775240219908644239793785735715026873347600343865175952761926303160
y2 = 927553665492332455747201965776037880757740193453592970025027978793976877002675564980949289727957565575433344219582*u + 1985150602287291935568054521177171638300868978215655730859378665066344726373823718423869104263333984641494340347905
G2 = E2(x2, y2)
#assert G2.order() == r

# Embedding degree
k = GF(r)(q).multiplicative_order()
assert k == 12

#assert G1.tate_pairing(G1, r, k, q) == 1
#assert G2.tate_pairing(G2, r, k, q) == 1

# G₁ ⊂ E(F)
# G₂ ⊂ E'(F₂)
# But the pairing function can only operate on curves from the same curve
# These points are chosen with mapping to F₁₂ called a sextic twist
# See https://hackmd.io/@benjaminion/bls12-381
# https://github.com/zebra-lucky/python-bls/blob/master/bls_py/ec.py

#value = GF(r)(110)
#assert (G1.tate_pairing(int(value) * G2, r, k) ==
#        (int(value) * G1).tate_pairing(G2, r, k))

