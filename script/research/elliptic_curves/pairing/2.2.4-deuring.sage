import math
def hasse_interval(q):
    interval = (q + 1 - 2 * sqrt(q)).n(), (q + 1 + 2 * sqrt(q)).n()
    return math.ceil(interval[0]), math.floor(interval[1])

q = 23
K = GF(q)

low, high = hasse_interval(23)

for i in range(100):
    a = K.random_element()
    b = K.random_element()

    try:
        E = EllipticCurve(K, [a, b])
    except:
        continue

    assert E.cardinality() >= low
    assert E.cardinality() <= high

