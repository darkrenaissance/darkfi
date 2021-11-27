# We'll use y^2 = x^3 + 3 for our curve, over F_101
p = 101
F = FiniteField(p)
R.<x_f> = F[]
E = EllipticCurve(F, [0, 3])
print(E)

e_points = E.points()
# Our generator G_1 = E(1, 2)
G_1 = e_points[1]

# Let's find a subgroup with this generator.
print(f"Finding a subgroup with generator {G_1} ...")
r = 1
elem = G_1
while True:
    elem += G_1
    r += 1
    if elem == e_points[0]:
        break

print(f"Found subgroup of order r={r} using generator {G_1}")

# Now let's find the embedding degree.
# The embedding degree is the smallest k such that r|p^k - 1
# In other words: p^k == 1 mod r
k = 1
print(f"Finding embedding degree for {p}^k mod {r} ...")
while True:
    if p ^ k % r == 1:
        break
    k += 1

print(f"Found embedding degree: k={k}")

# Our extension field. The polynomial x^2+2 is irreducible in F_101.
F2.<u> = F.extension(x_f^2+2, 'u')
assert u^2 == -2
print(F2)
E2 = EllipticCurve(F2, [0, 3])
print(E2)
# One of the generators for this curve we can use is (36, 31u)
G_2 = E2(36, 31*u)

# Now we build the trusted setup. The SRS is a list of EC points
# parameterized by a randomly generated secret number s.
# According to the PLONK protocol paper, a circuit with n gates requires
# an SRS with at least n+5 elements.

# We choose 2 as our random number for demo purposes.
s = 2
# Our circuit will have 4 gates.
n_gates = 4

SRS = []
for i in range(0, n_gates+3):
	SRS.append(s^i * G_1)
for i in range(0, 2):
	SRS.append(s^i * G_2)

# Composing our circuit. We'll test a^2 + b^2 = c^2:
# x_1 * x_1 = x_2
# x_3 * x_3 = x_4
# x_5 * x_5 = x_6
# x_2 + x_4 = x_6
#
# In order to satisfy these constraints, we need to supply six numbers
# as wire values that make all of the equations correct.
# e.g. x=(3, 9, 4, 16, 5, 25) would work.

# A full PLONK gate looks like this:
# (q_L)*a + (q_R)*b + (q_O)*c + (q_M)*a*b + q_C = 0
#
# Where a, b, c are the left, right, output wires of the gate.
#
# a + b = c  ---> q_L=1 , q_R=1, q_O=-1, q_M=0, q_C = 0
# a * b = c  ---> q_O=-1, q_M=1, and the rest = 0
#
# To bind a variable to a public value:
# q_R = q_O = q_M = 0
# q_L = 1
# q_C = public_value
#
# Considering all inputs as private, we get these four PLONK gates
# representing our circuit:
# 0*a_1 + 0*b_1 + (-1)*c_1 + 1*a_1*b_1 + 0 = 0    (a_1 * b_1 = c_1)
# 0*a_2 + 0*b_2 + (-1)*c_2 + 1*a_2*b_2 + 0 = 0    (a_2 * b_2 = c_2)
# 0*a_3 + 0*b_3 + (-1)*c_3 + 1*a_3*b_3 + 0 = 0    (a_3 * b_3 = c_3)
# 1*a_4 + 1*b_4 + (-1)*c_4 + 0*a_4*b_4 + 0 = 0    (a_4 + b_4 = c_4)
#
# So let's test with (3, 4, 5)
# a_i (left) values will be (3, 4, 5, 9)
# b_i (right) values will be (3, 4, 5, 16)
# c_i (output) values will be (9, 16, 25, 25)

# Selectors
q_L = vector([0, 0, 0, 1])
q_R = vector([0, 0, 0, 1])
q_O = vector([-1, -1, -1, -1])
q_M = vector([1, 1, 1, 0])
q_C = vector([0, 0, 0, 0])
# Assignments
a = vector([3, 4, 5, 9])
b = vector([3, 4, 5, 16])
c = vector([9, 16, 25, 25])

# Roots of Unity.
# The vectors for our circuit and assignment are all length 4, so the domain
# for our polynomial interpolation must have at least four elements.
roots_of_unity = []
F_r = FiniteField(r)
for i in F_r:
	if i^4 == 1:
		roots_of_unity.append(i)

omega_0 = roots_of_unity[0]
omega_1 = roots_of_unity[1]
omega_2 = roots_of_unity[3]
omega_3 = roots_of_unity[2]

# Cosets
# k_1 not in H, k_2 not in H nor k_1H
k_1 = 2
k_2 = 3
H = [omega_0, omega_1, omega_2, omega_3]
k1H = [H[0]*k_1, H[1]*k_1, H[2]*k_1, H[3]*k_1]
k2H = [H[0]*k_2, H[1]*k_2, H[2]*k_2, H[3]*k_2]
print("Polynomial interpolation using roots of unity")
print(f"H:   {H}")
print(f"k1H: {k1H}")
print(f"k2H: {k2H}")

# Interpolating using the Roots of Unity
# The interpolated polynomial will be degree-3 and have the form:
# f_a(x) = d + c*x + b*x^2 + a*x^3
# f_a(1) = 3, f_a(4) = 4, f_a(16) = 5, f_a(13) = 9
# Note that the above x is H (the omegas)
#
# This gives a system of equations:
# f(1)  = d + c*1 + b*1^2 + a*1^3 = 3
# f(4)  = d + c*4 + b*4^2 + a*4^3 = 4
# f(16) = d + c*16 + b*16^2 + a*16^3 = 5
# f(13) = d + c*13 + b*13^2 + a*13^3 = 9

# We can rewrite it as a matrix equation and solve by computing
# an inverse matrix.
def inverse_matrix(c):
	return Matrix([
		[c[0]^0, c[0]^1, c[0]^2, c[0]^3],
		[c[1]^0, c[1]^1, c[1]^2, c[1]^3],
		[c[2]^0, c[2]^1, c[2]^2, c[2]^3],
		[c[3]^0, c[3]^1, c[3]^2, c[3]^3],
	])^-1

# Now we can find polynomial f_a by multiplying the vector a=(3,4,5,9) by
# the interpolation matrix.
f_a_coeffs = inverse_matrix(H) * a
f_b_coeffs = inverse_matrix(H) * b
f_c_coeffs = inverse_matrix(H) * c
q_L_coeffs = inverse_matrix(H) * q_L
q_R_coeffs = inverse_matrix(H) * q_R
q_O_coeffs = inverse_matrix(H) * q_O
q_M_coeffs = inverse_matrix(H) * q_M
q_C_coeffs = inverse_matrix(H) * q_C

# The copy constraints involving left, right, output values are encoded as
# polynomials S_sigma_1, S_sigma_2, S_sigma_3 using the cosets we found
# earlier. The roots of unity H are used to label entries in vector a,
# the elements of k1H are used to label entries in vector b, and vector c is
# labeled by the elements of k2H.
print("Copy constraints:")
print(f"a: {a}")
print(f"b: {b}")
print(f"c: {c}")
# a1 = b1, a2 = b2, a3 = b3, a4 = c1
sigma_1 = vector([k1H[0], k1H[1], k1H[2], k2H[0]])
print(f"sigma_1: {sigma_1}")
# b1 = a1, b2 = a2, b3 = a3, b4 = c2
sigma_2 = vector([H[0], H[1], H[2], k2H[1]])
print(f"sigma_2: {sigma_2}")
# c1 = a4, c2 = b4, c3 = c4, c4 = c3
sigma_3 = vector([H[3], k1H[3], k2H[3], k2H[2]])
print(f"sigma_3: {sigma_3}")

S_sigma_1_coeffs = inverse_matrix(H) * sigma_1
S_sigma_2_coeffs = inverse_matrix(H) * sigma_2
S_sigma_3_coeffs = inverse_matrix(H) * sigma_3
