# See also https://cp-algorithms.com/algebra/fft.html

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

f = 6*X^7 + 7*X^5 + 3*X^2 + X

def fft(F):
    print(f"fft({F})")
    # On the first invocation:
    #assert len(F) == n
    N = len(F)
    if N == 1:
        print("  returning 1")
        return F

    omega_prime = omega^(n/N)
    assert omega_prime^(n - 1) != 1
    assert omega_prime^N == 1
    # Split into even and odd powers of X
    F_e = [a for a in F[::2]]
    print("  Evens:", F_e)
    F_o = [a for a in F[1::2]]
    print("  Odds:", F_o)

    y_e, y_o = fft(F_e), fft(F_o)
    print(f"y_e = {y_e}, y_o = {y_o}")
    y = [0] * N
    for j in range(N / 2):
        y[j] = y_e[j] + omega_prime^j * y_o[j]
        y[j + N / 2] = y_e[j] - omega_prime^j * y_o[j]
    print(f"  returning y = {y}")
    return y

print("f =", f)
evals = fft(list(f))
print("evals =", evals)
print("{omega^i : i in {0, 1, ..., n - 1}} =", [omega^i for i in range(n)])
evals2 = [f(omega^i) for i in range(n)]
print("{f(omega^i) for all omega^i} =", evals2)
assert evals == evals2

