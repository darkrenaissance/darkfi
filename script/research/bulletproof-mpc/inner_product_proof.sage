load('../mpc/curve.sage')
load('proof.sage')
load('transcript.sage')

n = 2
Q = [CurvePoint.generator()]
H = [CurvePoint.generator() for i in range(0,n)]
G = [CurvePoint.generator() for i in range(0,n)]

a = [1, 2]
b = [2, 4]
c = [sum([a*b for a, b in zip(a, b)])]
print('c: {}'.format(c))
y_inv = K(1)

G_factors = [K(1)]*n
H_factors = [y_inv**i for i in range(0,n)]

b_prime = [b*y for b, y in  zip(b, H_factors)]
a_prime = a.copy()

transcript = Transcript('bulletproof')
proof = Proof(transcript, Q, G_factors, H_factors, G, H, a, b)
g_a_prime = CurvePoint.msm(G, a_prime)
h_b_prime = CurvePoint.msm(H, b_prime)
q_c = CurvePoint.msm(Q, c)
P_res =  sum([g_a_prime, h_b_prime, q_c])
print('g_a_prime: {}'.format(g_a_prime))
print('h_b_prime: {}'.format(h_b_prime))
print('q_c: {}'.format(q_c))
print('P: {}'.format(P_res))
verifier = Transcript('bulletproof')
pp, p, _ = proof.verify(n, verifier, G_factors, H_factors, P_res, Q, G, H)
