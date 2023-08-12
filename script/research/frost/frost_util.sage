# Utility functions for the FROST module.
from hashlib import sha256


def scalar_to_bytes(a):
    int_repr = ZZ(a)
    byte_repr = int(int_repr).to_bytes(32, byteorder="big")
    return byte_repr


def point_to_bytes(P):
    if P.is_zero():
        return b"\x00"

    x_bytes = int(ZZ(P[0])).to_bytes(32, byteorder="big")
    y_bytes = int(ZZ(P[1])).to_bytes(32, byteorder="big")
    return x_bytes + y_bytes


def hash_domain(domain, *args):
    concat = domain.encode() + b"".join(str(arg).encode() for arg in args)
    return int(sha256(concat).hexdigest(), 16)


def H1(*args):
    return hash_domain("H1", *args)


def H2(*args):
    return hash_domain("H2", *args)


def H3(*args):
    return hash_domain("H3", *args)


def H4(*args):
    return hash_domain("H4", *args)


def H5(*args):
    return hash_domain("H5", *args)


# Serialize the group commitment list
def encode_group_commitment_list(commit_list):
    encoded_group_commitment = b""
    for (ident, hiding_nonce_commit, binding_nonce_commit) in commit_list:
        enc_commit = scalar_to_bytes(ident) \
            + point_to_bytes(hiding_nonce_commit) \
            + point_to_bytes(binding_nonce_commit)

        encoded_group_commitment = encoded_group_commitment + enc_commit

    return encoded_group_commitment
