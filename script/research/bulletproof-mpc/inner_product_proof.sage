load('../mpc/curve.sage')
load('proof.sage')
load('transcript.sage')

n = 4
Q = [CurvePoint.random()]
H = [CurvePoint.random() for i in range(0,n)]
G = [CurvePoint.random() for i in range(0,n)]

a = [K(random.randint(0,p)) for _ in range(0,n)]
b = [K(random.randint(0,p)) for _ in range(0,n)]
c = [sum([a*b for a, b in zip(a, b)])]
y_inv = K(random.randint(0,p))

G_factors = [K(1)]*n
H_factors = [y_inv**i for i in range(0,n)]

b_prime = [b*y for b, y in  zip(b, H_factors)]
a_prime = a.copy()

transcript = Transcript('bulletproof')
proof = Proof(transcript, Q, G_factors, H_factors, G, H, a, b)

P_res =  sum([CurvePoint.msm(G, a_prime), CurvePoint.msm(H, b_prime), CurvePoint.msm(Q, c)])
verifier = Transcript('bulletproof')
pp, p, _ = proof.verify(n, verifier, G_factors, H_factors, P_res, Q, G, H)
