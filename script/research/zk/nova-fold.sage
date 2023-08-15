p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
Fp = GF(p)
E = EllipticCurve(Fp, (0, 5))
G = [E.random_point() for _ in range(5)]
H = E.random_point()
q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
assert H.order() == q
K = GF(q)

# def foo(s, x, y):
#   if s:
#       return x * y
#   else:
#       return x + y

# z = foo(s, x, y)

# Arithmetization for:
# sxy + (s - 1)(x + y) - z = 0
# s(s - 1) = 0

def build_witness(x, y, s):
    var_1 = K(1)
    var_x = K(x)
    var_y = K(y)
    var_s = K(s)

    var_xy = var_x*var_y
    var_sxy = var_s*var_xy
    # w1 = (s - 1)(x + y)
    var_w1 = (var_s - 1)*(var_x + var_y)
    var_z = var_sxy + var_w1

    W = [var_x, var_y, var_s, var_xy, var_sxy, var_w1]
    X = [var_z, var_1]
    return W, X

W1, X1 = build_witness(4, 6, 1)
W2, X2 = build_witness(2, 3, 0)
S1 = vector(W1 + X1)
S2 = vector(W2 + X2)

# Circuit
L = matrix([
    [1, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 1, 0, 0, 0, 0, 0],
    [0, 0, 1, 0, 0, 0, 0, -1],
    [0, 0, 0, 0, 1, 1, 0, 0],
    [0, 0, 1, 0, 0, 0, 0, 0],
])
R = matrix([
    [0, 1, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 1, 0, 0, 0, 0],
    [1, 1, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 1],
    [0, 0, 1, 0, 0, 0, 0, -1],
])
O = matrix([
    [0, 0, 0, 1, 0, 0, 0, 0],
    [0, 0, 0, 0, 1, 0, 0, 0],
    [0, 0, 0, 0, 0, 1, 0, 0],
    [0, 0, 0, 0, 0, 0, 1, 0],
    [0, 0, 0, 0, 0, 0, 0, 0],
])

def hadamard_prod(A, B):
    result = []
    for a_i, b_i in zip(A, B):
        result.append(a_i * b_i)
    return vector(K, result)

def commit(T):
    r = K.random_element()
    C = r*H
    for t, G_i in zip(T, G):
        C += t*G_i
    return C

assert hadamard_prod(L*S1, R*S1) == O*S1
assert hadamard_prod(L*S2, R*S2) == O*S2

# Now lets combine both proofs together
μ1, μ2 = 1, 1
E1, E2 = vector([0]*5), vector([0]*5)

com_E1 = commit(E1)
com_E2 = commit(E2)

com_W1 = commit(W1)
com_W2 = commit(W2)

transcript = [
    L, R, O,
    com_E1, μ1, com_W1, X1,
    com_E2, μ2, com_W2, X2
]

# First send over the cross term
T = (hadamard_prod(L*S1, R*S2) + hadamard_prod(L*S2, R*S1)
     - μ1*O*S2 - μ2*O*S1)
# Send to verifier
com_T = commit(T)
transcript += [com_T]

#######
# Verifier
r = K.random_element()
μ = μ1 + r*μ2
X = vector(X1) + r*vector(X2)
com_W = com_W1 + r*com_W2
com_E = com_E1 + r*com_T + r^2*com_E2
transcript += [r]
#######

W = vector(W1) + r*vector(W2)
E = E1 + r*T + r^2*E2
S = vector(list(W) + list(X))
assert hadamard_prod(L*S, R*S) == μ*O*S + E

witness = (W, E)
proof = (L, R, O, com_E, μ, com_W, X)

