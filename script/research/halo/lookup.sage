import random

q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
K = GF(q)
P.<X> = K[]

def get_omega():
    generator = K(5)
    assert (q - 1) % 2^32 == 0
    # Root of unity
    t = (q - 1) / 2^32
    omega = generator**t

    assert omega != 1
    assert omega^(2^16) != 1
    assert omega^(2^31) != 1
    assert omega^(2^32) == 1

    return omega

# Order of this element is 2^32
omega = get_omega()
k = 3
n = 2^k
omega = omega^(2^32 / n)
assert omega^n == 1

A = 2
S = [3, 2, 3, 4, 5]

# We are checking that A is in S

# Extend S with random values so it is equal to n
S += [110, 110]
# Last value is impossible to access with the permutation loop
# so it is unused.
assert len(S) == n - 1

# Extend A with dummy values (the other values in S)
# We waste an entire column with this check
# but multiple lookup checks can be combined in a single column
# using a 'tag'
A = [A] + [3, 5, 3, 110, 3, 110]
assert len(A) == len(S)

# Random permutations of A and S
A_prime = [2, 3, 3, 3, 5, 110, 110]
S_prime = [2, 3, 4, 3, 5, 110, 110]
# First values must be the same
assert A_prime[0] == S_prime[0]
# Observe that for the values that do not match S_prime then
# they are equal to the previous value.
for i in range(len(A_prime)):
    assert A_prime[i] == S_prime[i] or A_prime[i] == A_prime[i - 1]

beta = K.random_element()
gamma = K.random_element()

# Last row is unused

permutation_points = [(omega^0, K(1))]
for i in range(1, n):
    x = omega^i

    y = K(1)
    for j in range(i):
        y *= ((A[j] + beta) * (S[j] + gamma) /
              ((A_prime[j] + beta) * (S_prime[j] + gamma)))

Z = P.lagrange_polynomial(permutation_points)
assert Z(omega^0) == 1
assert Z(omega^(n - 1)) == 1
assert omega^n == omega^0

# So now we have proved that A is a permutation of A_prime,
# and S is a permutation of S_prime.

