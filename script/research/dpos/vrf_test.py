from ouroboros.vrf import VRF, verify

seed='seed'
m=234234234

vrf = VRF(seed)
y, pi  = vrf.sign(m)

assert vrf.verify(m, y, pi), 'verification failed'
assert verify(m, y, pi, vrf.pk, vrf.g), 'verification failed'