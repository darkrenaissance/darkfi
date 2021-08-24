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

