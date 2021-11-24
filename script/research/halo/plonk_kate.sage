# We'll use y^2 = x^3 + 3 for our curve, over F_101
p = 101
F = FiniteField(p)
R.<x> = F[]
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
F2.<u> = F.extension(x^2+2, 'u')
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
n = 4

SRS = []
for i in range(0, n+3):
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
