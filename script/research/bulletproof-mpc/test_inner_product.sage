load('proof.sage')
load('transcript.sage')

n = 2
Q = [CurvePoint.generator()*1]
H = [CurvePoint.generator()*1 for i in range(0,n)]
G = [CurvePoint.generator()*1 for i in range(0,n)]

a = [1, 2]
b = [2, 4]
c = [sum([a*b for a, b in zip(a, b)])]
y_inv = K(1)
G_factors = [K(1)]*n
H_factors = [y_inv**i for i in range(0,n)]

b_prime = [b*y for b, y in  zip(b, H_factors)]
a_prime = a.copy()

transcript = Transcript('bulletproof')

proof = Proof(transcript, Q, G_factors, H_factors, G, H, a, b)
ga_prime = CurvePoint.msm(G, a_prime)
print('ga_prime: {}'.format(ga_prime))
hb_prime = CurvePoint.msm(H, b_prime)
qc = CurvePoint.msm(Q, c)
P_res =  sum([ga_prime, hb_prime, qc])
print('P_res: {}'.format(P_res))
verifier = Transcript('bulletproof')
pp, p, _ = proof.verify(n, verifier, G_factors, H_factors, P_res, Q, G, H)
