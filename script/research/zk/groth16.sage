q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
K = GF(q)
P.<X> = K[]

# def foo(s, x, y):
#   if s:
#       return x * y
#   else:
#       return x + y

# z = foo(s, x, y)

# Arithmetization for:
# sxy + (s - 1)(x + y) - z = 0
# s(s - 1) = 0

var_1 = K(1)
var_x = K(4)
var_y = K(6)
var_s = K(1)

var_xy = var_x*var_y
var_sxy = var_s*var_xy
# w1 = (s - 1)(x + y)
var_w1 = (var_s - 1)*(var_x + var_y)
var_z = var_sxy + var_w1

# There are n = 8 variables
# i =         0      1      2      3      4        5        6      7
S = vector([var_1, var_x, var_y, var_s, var_xy, var_sxy, var_w1, var_z])

# Row 1
# var_x * var_y == var_xy
L_1 = vector([0, 1, 0, 0, 0, 0, 0, 0])
R_1 = vector([0, 0, 1, 0, 0, 0, 0, 0])
O_1 = vector([0, 0, 0, 0, 1, 0, 0, 0])
assert (L_1*S) * (R_1*S) == (O_1*S)

# Row 2
# var_s * var_xy == var_sxy
L_2 = vector([0, 0, 0, 1, 0, 0, 0, 0])
R_2 = vector([0, 0, 0, 0, 1, 0, 0, 0])
O_2 = vector([0, 0, 0, 0, 0, 1, 0, 0])
assert (L_2*S) * (R_2*S) == (O_2*S)

# Row 3
# (var_s - 1) * (var_x + var_y) == var_w1
L_3 = vector([-1, 0, 0, 1, 0, 0, 0, 0])
R_3 = vector([ 0, 1, 1, 0, 0, 0, 0, 0])
O_3 = vector([ 0, 0, 0, 0, 0, 0, 1, 0])
assert (L_3*S) * (R_3*S) == (O_3*S)

# Row 4
# Here we want to check z = sxy + w1, but we need every row to have
# at least one multiplication, so we just use the constant 1 for the RHS.
# (var_sxy + var_w1) * var_1 == var_z
L_4 = vector([0, 0, 0, 0, 0, 1, 1, 0])
R_4 = vector([1, 0, 0, 0, 0, 0, 0, 0])
O_4 = vector([0, 0, 0, 0, 0, 0, 0, 1])
assert (L_4*S) * (R_4*S) == (O_4*S)

# Row 5
# Boolean check for s.
# var_s * (var_s - var_1) == 0
L_5 = vector([ 0, 0, 0, 1, 0, 0, 0, 0])
R_5 = vector([-1, 0, 0, 1, 0, 0, 0, 0])
O_5 = vector([ 0, 0, 0, 0, 0, 0, 0, 0])
assert (L_5*S) * (R_5*S) == (O_5*S)

L = matrix([L_1, L_2, L_3, L_4, L_5])
R = matrix([R_1, R_2, R_3, R_4, R_5])
O = matrix([O_1, O_2, O_3, O_4, O_5])

def hadamard_prod(A, B):
    result = []
    for a_i, b_i in zip(A, B):
        result.append(a_i * b_i)
    return vector(result)

assert hadamard_prod(L*S, R*S) == O*S

# Now extract columns from matrices
L_i_1, L_i_2, L_i_3, L_i_4, L_i_5, L_i_6, L_i_7, L_i_8 = (
    L[:,0], L[:,1], L[:,2], L[:,3], L[:,4], L[:,5], L[:,6], L[:,7]
)
R_i_1, R_i_2, R_i_3, R_i_4, R_i_5, R_i_6, R_i_7, R_i_8 = (
    R[:,0], R[:,1], R[:,2], R[:,3], R[:,4], R[:,5], R[:,6], R[:,7]
)
O_i_1, O_i_2, O_i_3, O_i_4, O_i_5, O_i_6, O_i_7, O_i_8 = (
    O[:,0], O[:,1], O[:,2], O[:,3], O[:,4], O[:,5], O[:,6], O[:,7]
)

l_1_X = P.lagrange_polynomial((i, l_i_1) for i, (l_i_1,) in enumerate(L_i_1))
l_2_X = P.lagrange_polynomial((i, l_i_2) for i, (l_i_2,) in enumerate(L_i_2))
l_3_X = P.lagrange_polynomial((i, l_i_3) for i, (l_i_3,) in enumerate(L_i_3))
l_4_X = P.lagrange_polynomial((i, l_i_4) for i, (l_i_4,) in enumerate(L_i_4))
l_5_X = P.lagrange_polynomial((i, l_i_5) for i, (l_i_5,) in enumerate(L_i_5))
l_6_X = P.lagrange_polynomial((i, l_i_6) for i, (l_i_6,) in enumerate(L_i_6))
l_7_X = P.lagrange_polynomial((i, l_i_7) for i, (l_i_7,) in enumerate(L_i_7))
l_8_X = P.lagrange_polynomial((i, l_i_8) for i, (l_i_8,) in enumerate(L_i_8))
for i, row in enumerate(L):
    assert l_1_X(i) == row[0]
    assert l_2_X(i) == row[1]
    assert l_3_X(i) == row[2]
    assert l_4_X(i) == row[3]
    assert l_5_X(i) == row[4]
    assert l_6_X(i) == row[5]
    assert l_7_X(i) == row[6]
    assert l_8_X(i) == row[7]
# l₁(X) represents var_1 which is i = 0
# X=0 is row 1
assert l_1_X(0) == L_1[0]
# X=4 is row 5
assert l_1_X(4) == L_5[0]
# l₄(X) represents var_s which is i = 3
# X=2 is row 3
assert l_4_X(2) == L_3[3]

r_1_X = P.lagrange_polynomial((i, r_i_1) for i, (r_i_1,) in enumerate(R_i_1))
r_2_X = P.lagrange_polynomial((i, r_i_2) for i, (r_i_2,) in enumerate(R_i_2))
r_3_X = P.lagrange_polynomial((i, r_i_3) for i, (r_i_3,) in enumerate(R_i_3))
r_4_X = P.lagrange_polynomial((i, r_i_4) for i, (r_i_4,) in enumerate(R_i_4))
r_5_X = P.lagrange_polynomial((i, r_i_5) for i, (r_i_5,) in enumerate(R_i_5))
r_6_X = P.lagrange_polynomial((i, r_i_6) for i, (r_i_6,) in enumerate(R_i_6))
r_7_X = P.lagrange_polynomial((i, r_i_7) for i, (r_i_7,) in enumerate(R_i_7))
r_8_X = P.lagrange_polynomial((i, r_i_8) for i, (r_i_8,) in enumerate(R_i_8))
for i, row in enumerate(R):
    assert r_1_X(i) == row[0]
    assert r_2_X(i) == row[1]
    assert r_3_X(i) == row[2]
    assert r_4_X(i) == row[3]
    assert r_5_X(i) == row[4]
    assert r_6_X(i) == row[5]
    assert r_7_X(i) == row[6]
    assert r_8_X(i) == row[7]
# r₁(X) represents var_1 which is i = 0
# X=4 is row 5
assert r_1_X(4) == R_5[0]

o_1_X = P.lagrange_polynomial((i, o_i_1) for i, (o_i_1,) in enumerate(O_i_1))
o_2_X = P.lagrange_polynomial((i, o_i_2) for i, (o_i_2,) in enumerate(O_i_2))
o_3_X = P.lagrange_polynomial((i, o_i_3) for i, (o_i_3,) in enumerate(O_i_3))
o_4_X = P.lagrange_polynomial((i, o_i_4) for i, (o_i_4,) in enumerate(O_i_4))
o_5_X = P.lagrange_polynomial((i, o_i_5) for i, (o_i_5,) in enumerate(O_i_5))
o_6_X = P.lagrange_polynomial((i, o_i_6) for i, (o_i_6,) in enumerate(O_i_6))
o_7_X = P.lagrange_polynomial((i, o_i_7) for i, (o_i_7,) in enumerate(O_i_7))
o_8_X = P.lagrange_polynomial((i, o_i_8) for i, (o_i_8,) in enumerate(O_i_8))
for i, row in enumerate(O):
    assert o_1_X(i) == row[0]
    assert o_2_X(i) == row[1]
    assert o_3_X(i) == row[2]
    assert o_4_X(i) == row[3]
    assert o_5_X(i) == row[4]
    assert o_6_X(i) == row[5]
    assert o_7_X(i) == row[6]
    assert o_8_X(i) == row[7]

l_X = vector([l_1_X, l_2_X, l_3_X, l_4_X, l_5_X, l_6_X, l_7_X, l_8_X])
r_X = vector([r_1_X, r_2_X, r_3_X, r_4_X, r_5_X, r_6_X, r_7_X, r_8_X])
o_X = vector([o_1_X, o_2_X, o_3_X, o_4_X, o_5_X, o_6_X, o_7_X, o_8_X])

# Evaluate each row
for q in range(5):
    lhs = sum(S[i]*l_X[i](q) for i in range(8))
    rhs = sum(S[i]*r_X[i](q) for i in range(8))
    out = sum(S[i]*o_X[i](q) for i in range(8))
    assert lhs*rhs == out

# So this and the matrix form are both equivalent
t = (S*l_X) * (S*r_X) - S*o_X
for i in range(5):
    assert t(i) == 0

z = (X - 0)*(X - 1)*(X - 2)*(X - 3)*(X - 4)
h, rem = t.quo_rem(z)
assert rem == 0

