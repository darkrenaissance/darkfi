p = 2^31 - 1
q = 2^61 - 1
assert is_prime(p)
assert is_prime(q)
n = p * q
# Order of the multiplicative group for n
# phi = (p - 1) * (q - 1)
K = IntegerModRing(n)

A_0 = K(5)

c_0 = random_prime(2^12)
A_1 = A_0^c_0

c_1 = random_prime(2^12)
A_2 = A_1^c_1

c_2 = random_prime(2^12)
W_3 = A_2
A_3 = A_2^c_2

c_3 = random_prime(2^12)
W_4 = W_3^c_3
A_4 = A_3^c_3

c_4 = random_prime(2^12)
W_5 = W_4^c_4
A_5 = A_4^c_4

assert W_5^c_2 == A_5
assert A_5 == A_0^(c_0 * c_1 * c_2 * c_3 * c_4)
assert W_5 == A_0^(c_0 * c_1       * c_3 * c_4)

