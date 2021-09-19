q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
K = GF(q)
a = K(0x00)
b = K(0x05)
E = EllipticCurve(K, (a, b))
G = E(0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000000, 0x02)

p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
assert E.order() == p
Scalar = GF(p)

k = 3
n = 2^k

a = [Scalar(110), Scalar(56),  Scalar(89), Scalar(6543),
     Scalar(2),   Scalar(110), Scalar(44), Scalar(78)]

x = Scalar.random_element()
b = [x^i for i in range(n)]

G = [E.random_element(), E.random_element(), E.random_element(),
     E.random_element(), E.random_element(), E.random_element(),
     E.random_element(), E.random_element()]

assert len(a) == len(b) == len(G) == n

# Dot product
def dot(x, y):
    result = None
    for x_i, y_i in zip(x, y):
        if result is None:
            result = int(x_i) * y_i
        else:
            result += int(x_i) * y_i
    return result

challenges = []
commits = []

# Iterate k times where n = 2^k
for k in range(k, 0, -1):
    half = 2^(k - 1)
    assert half * 2 == len(a)

    L = dot(a[half:], G[:half])
    R = dot(a[:half], G[half:])
    #z_L = dot(a[half:], b[:half])
    #z_R = dot(a[:half], b[half:])
    commits.append((L, R))

    challenge = Scalar.random_element()
    challenges.append(challenge)

    a = [a[i] + challenge^-1 * a[half + i] for i in range(half)]
    G = [G[i] + int(challenge) * G[half + i] for i in range(half)]
    assert len(a) == len(G) == half

    if k == 0:
        print("Last round")
        assert len(a[-1]) == 1
        assert len(G[-1]) == 1

