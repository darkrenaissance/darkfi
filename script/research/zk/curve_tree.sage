import hashlib
from collections import namedtuple

# Your Funds Are Safu

p = [
    0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001,
    0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
]

# Pallas, Vesta
K = [GF(p_i) for p_i in p]
E = [EllipticCurve(K_i, (0, 5)) for K_i in K]
# Scalar fields
Scalar = [K[1], K[0]]

base_G = [E_i.gens()[0] for E_i in E]
assert all(base_G_i.order() == p_i for p_i, base_G_i in zip(reversed(p), base_G))

E1, E2 = 0, 1

gens = [
    [E[E1].random_point() for _ in range(5)],
    [E[E2].random_point() for _ in range(5)],
]

def hash_nodes(Ei, P1, P2, r):
    G1, G2, G3, G4, H = gens[Ei]

    (P1_x, P1_y), (P2_x, P2_y) = P1.xy(), P2.xy()

    v1G1 = int(P1_x) * G1
    v2G2 = int(P1_y) * G2
    v3G3 = int(P2_x) * G3
    v4G4 = int(P2_y) * G4
    rH   = int(r)    * H

    return v1G1 + v2G2 + v3G3 + v4G4 + rH

def hash_point(Ei, P, b):
    G1, G2, G3, G4, H = gens[Ei]
    x, y = P.xy()
    return int(x)*G1 + int(y)*G2 + int(b)*H

# You can ignore this particular impl.
# Just some rough code to illustrate the main concept.
# The proofs enforce these relations:
#
#   σ ∈ {0, 1}
#   C = x1  G1  + y1  G2 + x2 G3 + y2 G4 + rH
#   Ĉ = x_i G1  + y_i G2                 + bH
#
# where
#
#   x_i = { x1  if  σ = 0
#         { x2  if  σ = 1
#
#   y_i = { y1  if  σ = 0
#         { y2  if  σ = 1
#
# It is just a quick hackjob proof of concept and horribly inefficient
load("curve_tree_proofs.sage")
test_proof()

# Our tree is a height of D=3

def create_tree(C3):
    assert len(C3) == 2**3

    # j = 2
    C2 = []
    for i in range(4):
        C2_i = hash_nodes(E2, C3[2*i], C3[2*i + 1], 0)
        C2.append(C2_i)

    # j = 1
    C1 = []
    for i in range(2):
        C1_i = hash_nodes(E1, C2[2*i], C2[2*i + 1], 0)
        C1.append(C1_i)

    # j = 0 (root)
    C0 = hash_nodes(E2, C1[0], C1[1], 0)
    return C0

def create_path(C3):
    # To make things easier, we assume that our coin is
    # always on the left hand side of the tree.

    X3 = C3[1]
    X2 = hash_nodes(E2, C3[2], C3[3], 0)

    X1 = hash_nodes(
        E1,
        hash_nodes(E2, C3[4], C3[5], 0),
        hash_nodes(E2, C3[6], C3[7], 0),
        0
    )

    return (X3, X2, X1)

def main():
    coins = [E[E1].random_point() for _ in range(2**3)]
    root = create_tree(coins)

    path = create_path(coins)

    # Test the path works
    X3, X2, X1 = path
    C3 = coins[0]
    C2 = hash_nodes(
        E2,
        C3,
        X3,
        0
    )
    C1 = hash_nodes(
        E1,
        C2,
        X2,
        0
    )
    C0 = hash_nodes(
        E2,
        C1,
        X1,
        0
    )
    assert C0 == root

    # E1 point
    C3 = coins[0]

    Ĉ0 = root
    r0 = 0
    # Same as this:
    # Ĉ0 = hash_nodes(E1, C2, X2, 0)

    # j = 1
    b1 = int(Scalar[E2].random_element())
    Ĉ1 = hash_point(E2, C1, b1)

    C1_x, C1_y = C1.xy()
    X1_x, X1_y = X1.xy()

    proof1, public1 = make_proof(
        E2,
        ProofWitness(
            C1_x,
            C1_y,
            X1_x,
            X1_y,
            r0,
            b1,
            0
        )
    )
    public1.C = Ĉ0
    public1.D = Ĉ1

    assert verify_proof(E2, proof1, public1)

    # j = 2

    # Now we know that Ĉ1 is the root of a new subtree
    # But Ĉ1 ∈ E2, whereas we need to produce a blinded
    # Ĉ1 ∈ E1.
    # The reason this system uses curve cycles is because
    # EC arithmetic is efficient to represent.
    # We skip this part so assume these next to lines are
    # part of the previous proof.
    r1 = int(Scalar[E1].random_element())
    Ĉ1 = hash_nodes(E1, C2, X2, r1)
    ################################

    b2 = int(Scalar[E1].random_element())
    Ĉ2 = hash_point(E1, C2, b2)

    C2_x, C2_y = C2.xy()
    X2_x, X2_y = X2.xy()

    proof2, public2 = make_proof(
        E1,
        ProofWitness(
            C2_x,
            C2_y,
            X2_x,
            X2_y,
            r1,
            b2,
            0
        )
    )
    public2.C = Ĉ1
    public2.D = Ĉ2

    assert verify_proof(E1, proof2, public2)

    # j = 3

    # Same as before. We now have a randomized C2
    r2 = int(Scalar[E2].random_element())
    Ĉ2 = hash_nodes(E2, C3, X3, r2)
    #################################

    b3 = int(Scalar[E2].random_element())
    Ĉ3 = hash_point(E2, C3, b3)

    C3_x, C3_y = C3.xy()
    X3_x, X3_y = X3.xy()

    proof3, public3 = make_proof(
        E2,
        ProofWitness(
            C3_x,
            C3_y,
            X3_x,
            X3_y,
            r2,
            b3,
            0
        )
    )
    public3.C = Ĉ2
    public3.D = Ĉ3

    assert verify_proof(E2, proof3, public3)

    # Now just unblind Ĉ3

main()

