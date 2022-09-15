# for more info check Washington example 11.7
# https://github.com/narodnik/elliptic-curves-washington-solutions
p = 11
K = GF(p)
E = EllipticCurve(K, [-1, 1])
R.<x, y> = PolynomialRing(K)

n = 5
P = E(3, 6)
assert P.order() == 5
Q = P
inf = E(0)

# We are computing <P, P>_5
#   v_5 = f_5(P) / f_5(P)
# where
#   div(f_5) = n[P + R] - n[R]

D_P = ((1, P), (-1, inf))

# Random point for D_Q
R = E(0, 1)
D_Q = ((1, Q + R), (-1, R))

# n = 5 = 1 + 4
# so we perform one addition, two doublings and another addition

# Step 1
i = n
j = 0
k = 1
fj = fk = 1
# Compute f1 such that
#   div(f1) = [P + R] - [P] - [R] + [∞]
# But D_P = [(3, 6)] - [∞]
# so R = ∞, and hence D_1 = [P + ∞] - [P] - [∞] + [∞]
#                         = 0
# hence f1 = 1

# First valuations of f0, f1
(vj, vk) = (1, 1)

dydx = lambda x, y: (3*x^2 - 1) / (2*y)

print(f"i = {i}, j = {j}, k = {k}")
while i != 0:
    if i % 2 == 0:
        i /= 2

        # We are computing div(l) which is
        # the line between kP and kP
        kP = k*P
        m = dydx(kP[0], kP[1])
        c = kP[1] - m*kP[0]
        l = y - m*x - c
        # And now the vertical line through 2kP
        _2kP = 2*kP
        v = x - _2kP[0]

        f = l / v

        f_valuation = 1
        for d, X in D_Q:
            f_valuation *= f(X[0], X[1])^d
        vk = vk^2 * f_valuation
        print(f"  f = {f}")
        print(f"  vk = {vk}")

        k *= 2
    else:
        i -= 1

        if j + k == 1:
            assert k == 1
            # fj = fk
            # vj = vk
        else:
            # Interpolate jP and kP
            assert j != k
            jP = j*P
            kP = k*P
            print(f"{jP}, {kP}")
            if jP[0] != kP[0]:
                m = (jP[1] - kP[1]) / (jP[0] - kP[0])
                c = jP[1] - m*jP[0]
                l = y - m*x - c
            else:
                l = x - jP[0]
            # Vertical line through (j + k)P
            jkP = (j + k)*P
            if jkP != inf:
                v = x - jkP[0]
            else:
                v = K(1)

            f = l / v
            print(f"  f = {f}")
            
            f_valuation = 1
            for d, X in D_Q:
                f_valuation *= f(X[0], X[1])^d
            vj = vj * vk * f_valuation
            print(f"  vj = {vj}")

        j += k

    print(f"i = {i}, j = {j}, k = {k}")

modified_tate = vj^((p - 1) / n)
print(f"result = {modified_tate}")
