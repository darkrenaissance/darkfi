def is_normal(f):
    a, b, c = f
    return -a < b <= a

def is_reduced(f):
    a, b, c = f
    return is_normal(f) and (a < c or (a == c and b >= 0))

# Action of SL₂(ℤ) on a form (x y)ᵗ
# This also will always terminate on the final reduced form in a class
def reduce(f):
    a, b, c = f
    while not is_reduced((a, b, c)):
        if a > c or (a == c and b < 0):
            a, b, c = c, -b, a
        elif a < c:
            if b <= -a:
                a, b, c = a, b + 2*a, c + b + a
            else:
                assert b > a
                a, b, c = a, b - 2*a, c - b + a
        else:
            assert a == c and b >= 0
            a, b, c = a, b - 2*a, c - b + a
    return a, b, c

def lincong(a, b, m):
    # 1
    g, d, e = xgcd(a, m)
    assert g == gcd(a, m)
    assert d*a + e*m == g

    # 2
    q = floor(b/g)
    r = b % g

    # 3
    if r != 0:
        return None

    # 4
    μ = q*d % m
    υ = m/g

    return μ, υ

# Composition algo taken from chia class groups document
def compose(f1, f2):
    a, b, c = f1
    α, β, γ = f2

    # 1
    g =  (b + β)/2
    h = -(b - β)/2
    w = gcd([a, α, g])

    # 2
    j = w
    s = a/w
    t = α/w
    u = g/w

    # 3
    if (vals := lincong(t*u, h*u + s*c, s*t)) is None:
        return None
    μ, υ = vals

    # 4
    if (vals := lincong(t*υ, h - t*μ, s)) is None:
        return None
    λ, _ = vals

    # 5
    k = μ + υ*λ
    l = (k*t - h)/s
    m = (t*u*k - h*u - c*s)/(s*t)

    # 6
    A = s*t
    B = j*u - (k*t + l*s)
    C = k*l - j*m

    # 7
    f3 = A, B, C
    return reduce(f3)

# Class number = 2
d = -5
D = 4*d

e = (1, 0, 5)
a = (2, 2, 3)

print(compose(e, e))
print(compose(e, a))
print(compose(a, e))
print(compose(a, a))

