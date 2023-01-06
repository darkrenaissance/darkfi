ProofWitness = namedtuple("ProofWitness", [
    "v1", "v2", "v3", "v4", "r", "b", "σ"
])

class ProofPublic:

    def __init__(self):
        self.C = None
        self.D = None
        self.X = None
        self.Y = None
        self.Z = None

class ProofCommits:

    def __init__(self):
        self.v1 = None
        self.v2 = None
        self.v3 = None
        self.v4 = None
        self.r  = None
        self.b  = None
        self.σ  = None

        self.σ_G1    = None
        self.σ_G2    = None
        self.v3_G1   = None
        self.v4_G2   = None
        self.blind_x = None
        self.blind_y = None
        self.blind_z = None

        # Used by inner product
        self.C0 = None
        self.C1 = None

    def transcript(self):
        points = [
            self.v1, self.v2, self.v3, self.v4, self.r, self.b, #self.σ,
            self.σ_G1, self.σ_G2, self.v3_G1, self.v4_G2,
            self.blind_x, self.blind_y, self.blind_z, self.C0, self.C1
        ]
        assert all(P is not None for P in points)
        points = [P.xy() for P in points]
        return list(zip(*points))

class ProofResponses:

    def __init__(self):
        self.v1 = None
        self.v2 = None
        self.v3 = None
        self.v4 = None
        self.r  = None
        self.b  = None
        self.σ  = None

        self.blind_x = None
        self.blind_y = None
        self.blind_z = None

        self.txy = None

Proof = namedtuple("Proof", [
    "R", "s", "boolean_check"
])

RingProof = namedtuple("RingProof", [
    "c0", "s0", "s1"
])

def make_proof(Ei, witness):
    G1, G2, G3, G4, H = gens[Ei]
    S = Scalar[Ei]

    blind_x  = int(S.random_element())
    blind_y  = int(S.random_element())
    blind_xy = int(S.random_element())
    # We want that blind_xy + blind_z == witness.b
    blind_z  = int(S(witness.b - blind_xy))

    k_v1 = int(S.random_element())
    k_v2 = int(S.random_element())
    k_v3 = int(S.random_element())
    k_v4 = int(S.random_element())
    k_r  = int(S.random_element())
    k_b  = int(S.random_element())
    k_σ  = int(S.random_element())

    k_blind_x  = int(S.random_element())
    k_blind_y  = int(S.random_element())
    k_blind_xy = int(S.random_element())
    k_blind_z  = int(S.random_element())

    # Used for inner product
    k_t0 = int(S.random_element())
    k_t1 = int(S.random_element())

    R = ProofCommits()
    R.v1 = k_v1 * G1
    R.v2 = k_v2 * G2
    R.v3 = k_v3 * G3
    R.v4 = k_v4 * G4
    R.r  = k_r  * H
    R.b  = k_b  * H

    # Used for 2nd proof
    R.σ_G1     = k_σ  * G1
    R.σ_G2     = k_σ  * G2
    R.v3_G1    = k_v3 * G1
    R.v4_G2    = k_v4 * G2
    R.blind_x  = k_blind_x  * H
    R.blind_y  = k_blind_y  * H
    R.blind_xy = k_blind_xy * H
    R.blind_z  = k_blind_z  * H

    # σ (v1 - v3)
    # σ (v2 - v4)
    # (k_σ + c σ)(k_v1 - k_v3 + c*(v1 - v3))
    #
    # sage: var("k_σ c σ k_v1 k_v3 v1 v3")
    # sage: ((k_σ + c*σ)*(k_v1 - k_v3 + c*(v1 - v3))).expand().collect(c)
    # (v1*σ - v3*σ)*c^2 + (k_σ*v1 - k_σ*v3 + k_v1*σ - k_v3*σ)*c + k_v1*k_σ - k_v3*k_σ
    R.C0 = (
        (k_v3*k_σ - k_v1*k_σ) * G1 +
        (k_v4*k_σ - k_v2*k_σ) * G2 +
        k_t0 * H
    )
    R.C1 = (
        (k_σ*witness.v3 - k_σ*witness.v1 + k_v3*witness.σ - k_v1*witness.σ) * G1 +
        (k_σ*witness.v4 - k_σ*witness.v2 + k_v4*witness.σ - k_v2*witness.σ) * G2 +
        k_t1 * H
    )

    c = hash_scalar(Ei, R.transcript())

    s = ProofResponses()
    s.v1 = int( k_v1 + c*witness.v1 )
    s.v2 = int( k_v2 + c*witness.v2 )
    s.v3 = int( k_v3 + c*witness.v3 )
    s.v4 = int( k_v4 + c*witness.v4 )
    s.r  = int( k_r  + c*witness.r  )
    s.b  = int( k_b  + c*witness.b  )
    s.σ  = int( k_σ  + c*witness.σ  )

    s.blind_x  = int(k_blind_x  + c*blind_x)
    s.blind_y  = int(k_blind_y  + c*blind_y)
    s.blind_xy = int(k_blind_xy + c*blind_xy)
    s.blind_z  = int(k_blind_z  + c*blind_z)

    s.txy = c**2 * blind_xy + c * k_t1 + k_t0

    public = ProofPublic()
    public.X = ((witness.v3 - witness.v1) * G1 +
                (witness.v4 - witness.v2) * G2 +
                blind_x * H)
    public.Y = witness.σ * G1 + witness.σ * G2 + blind_y * H
    public.XY = (
        witness.σ * (witness.v3 - witness.v1) * G1 +
        witness.σ * (witness.v4 - witness.v2) * G2 +
        blind_xy * H
    )
    public.Z = witness.v1 * G1 + witness.v2 * G2 + blind_z * H
    assert witness.σ in (0, 1)
    if witness.σ == 0:
        assert public.XY == blind_xy * H
        assert (
            public.XY + public.Z
            ==
            witness.v1*G1 + witness.v2*G2 + (blind_xy + blind_z)*H
        )
    else:
        assert witness.σ == 1
        assert (
            public.XY
            == 
            (witness.v3 - witness.v1) * G1 +
            (witness.v4 - witness.v2) * G2 +
            blind_xy * H
        )
        assert (
            public.XY + public.Z
            ==
            witness.v3*G1 + witness.v4*G2 + (blind_xy + blind_z)*H
        )
        assert blind_xy + blind_z == witness.b

    P1 = public.Y
    P2 = public.Y - G1 - G2
    assert blind_y*H           == [P1, P2][witness.σ]
    if witness.σ == 0:
        assert blind_y*H           == P1
        assert blind_y*H - G1 - G2 == P2
    else:
        assert witness.σ == 1
        assert blind_y*H + G1 + G2 == P1
        assert blind_y*H           == P2
    boolean_check = make_ring_sig(Ei, [P1, P2], blind_y, int(witness.σ))
    assert verify_ring_sig(Ei, boolean_check, [P1, P2])

    return Proof(R, s, boolean_check), public

def make_ring_sig(Ei, public_keys, secret, j):
    H, S = gens[Ei][-1], Scalar[Ei]
    assert len(public_keys) == 2
    assert secret*H == public_keys[j]
    assert j in (0, 1)

    k0 = int(S.random_element())
    R0 = k0*H

    c1 = hash_scalar(Ei, R0.xy())
    s1 = int(S.random_element())
    R1 = s1*H - c1*public_keys[(j + 1) % 2]

    c0 = hash_scalar(Ei, R1.xy())
    s0 = k0 + c0*secret

    if j == 1:
        c0 = c1
        s0, s1 = s1, s0

    proof = RingProof(c0, s0, s1)
    return proof

def verify_ring_sig(Ei, proof, public_keys):
    H = gens[Ei][-1]
    S = Scalar[Ei]
    assert len(public_keys) == 2

    R1 = proof.s0*H - proof.c0*public_keys[0]
    c1 = hash_scalar(Ei, R1.xy())

    R2 = proof.s1*H - c1*public_keys[1]
    c2 = hash_scalar(Ei, R2.xy())

    return c2 == proof.c0

def verify_proof(Ei, proof, public):
    G1, G2, G3, G4, H = gens[Ei]
    S = Scalar[Ei]

    R, s = proof.R, proof.s

    c = hash_scalar(Ei, R.transcript())

    if (s.v1 * G1 +
        s.v2 * G2 +
        s.v3 * G3 +
        s.v4 * G4 +
        s.r  * H
        !=
        R.v1 + R.v2 + R.v3 + R.v4 + R.r + c*public.C
    ):
        return False

    # Now we want to prove that
    #   D = v1 G1 + v2 G2 + b H
    # or
    #   D = v3 G1 + v4 G2 + b H

    # We do this by checking:
    #   X = (v1 - v2)G + b_X H
    #   Y = σ G + b_Y H
    #   D = xy G + v2 G + b_D H
    #   σ ∈ {0, 1}

    #   X = (v1 - v2)G + b_X H
    if (s.v3 * G1 - s.v1 * G1 +
        s.v4 * G2 - s.v2 * G2 +
        s.blind_x * H
        !=
        R.v3_G1 - R.v1 + R.v4_G2 - R.v2 + R.blind_x + c*public.X
    ):
        return False

    #   Y = σ G + b_Y H
    if (s.σ * G1 + s.σ * G2 + s.blind_y * H
        !=
        R.σ_G1 + R.σ_G2 + R.blind_y + c*public.Y
    ):
        return False

    # Z = v1 G1 + v2 G2
    if (s.v1 * G1 + s.v2 * G2 + s.blind_z * H
        !=
        R.v1 + R.v2 + R.blind_z + c*public.Z
    ):
        return False

    # Inner product verification. We select either P1 or P2
    # prove D1 = x1 y1 G1 + b1 H
    if (s.σ*(s.v3 - s.v1)*G1 + s.σ*(s.v4 - s.v2)*G2 + s.txy*H
        !=
        c**2*public.XY + c*R.C1 + R.C0
    ):
        return False

    # check D is correct
    if public.D != public.XY + public.Z:
        return False

    # boolean check proof for s
    P1 = public.Y
    P2 = public.Y - G1 - G2
    if not verify_ring_sig(Ei, proof.boolean_check, [P1, P2]):
        return False

    return True

def hash_scalar(Ei, values):
    S = Scalar[Ei]

    hasher = hashlib.sha256()
    for value in values:
        hasher.update(str(value).encode())

    return S(int(hasher.hexdigest(), 16))

# Test proving system
def test_proof():
    G1, G2, G3, G4, H = gens[E1]
    S = Scalar[E1]

    P1, P2 = [E[E2].random_point() for _ in range(2)]
    (P1_x, P1_y), (P2_x, P2_y) = P1.xy(), P2.xy()
    r, b = [S.random_element() for _ in range(2)]

    C = hash_nodes(E1, P1, P2, r)
    # σ = 0 for P1, or σ = 1 for P2
    σ = S(1)
    D = hash_point(E1, P2, b)

    proof, public = make_proof(
        E1,
        ProofWitness(
            P1_x,
            P1_y,
            P2_x,
            P2_y,
            r,
            b,
            σ
        )
    )

    public.C = C
    public.D = D

    assert verify_proof(E1, proof, public)

    # Now try the other side too
    σ = S(0)
    D = hash_point(E1, P1, b)

    proof, public = make_proof(
        E1,
        ProofWitness(
            P1_x,
            P1_y,
            P2_x,
            P2_y,
            r,
            b,
            σ
        )
    )

    public.C = C
    public.D = D

    assert verify_proof(E1, proof, public)

    # Test the ring sigs too
    secret = int(S.random_element())
    P1 = secret*H
    P2 = E[E1].random_point()
    proof = make_ring_sig(E1, [P1, P2], secret, 0)
    assert verify_ring_sig(E1, proof, [P1, P2])
    # Also try in reverse
    P1, P2 = P2, P1
    proof = make_ring_sig(E1, [P1, P2], secret, 1)
    assert verify_ring_sig(E1, proof, [P1, P2])

    # Ring sigs is our boolean proof for σ
    σ = S(0)
    b = S.random_element()

    # Verifier only has P
    # We prove that σ ∈ {0, 1}
    P = σ*G1 + b*H
    # They can only make a ring signature on H
    # if σ is 0 or 1
    # P1 = P      represents σ = 0
    P1 = P
    # P2 = P - G1 represents σ = 1
    P2 = P - G1

    proof = make_ring_sig(E1, [P1, P2], b, 0)
    assert verify_ring_sig(E1, proof, [P1, P2])
    # Also try σ = 1
    σ = S(1)
    P = σ*G1 + b*H
    P1 = P
    P2 = P - G1
    proof = make_ring_sig(E1, [P1, P2], b, 1)
    assert verify_ring_sig(E1, proof, [P1, P2])

