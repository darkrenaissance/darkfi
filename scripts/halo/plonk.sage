q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
K = GF(q)

# The pallas and vesta curves are 2-adic. This means there is a large
# power of 2 subgroup within both of their fields.
# This function finds a generator for this subgroup within the field.
def get_omega():
    # Slower alternative:
    #     generator = K.multiplicative_generator()
    # Just hardcode the value here instead
    generator = K(5)
    assert (q - 1) % 2**32 == 0
    # Root of unity
    t = (q - 1) / 2**32
    omega = generator**t

    assert omega != 1
    assert omega**(2**16) != 1
    assert omega**(2**31) != 1
    assert omega**(2**32) == 1

    return omega

omega = get_omega()

