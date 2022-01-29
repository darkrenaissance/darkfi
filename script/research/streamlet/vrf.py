from msilib import type_string
from logger import Logger
import random as rnd
from  tate_bilinear_pairing import eta, ecc


def extended_euclidean_algorithm(a, b):
    """
    Returns a three-tuple (gcd, x, y) such that
    a * x + b * y == gcd, where gcd is the greatest
    common divisor of a and b.

    This function implements the extended Euclidean
    algorithm and runs in O(log b) in the worst case.
    """
    s, old_s = 0, 1
    t, old_t = 1, 0
    r, old_r = b, a

    while r != 0:
        quotient = old_r // r
        old_r, r = r, old_r - quotient * r
        old_s, s = s, old_s - quotient * s
        old_t, t = t, old_t - quotient * t

    return old_r, old_s, old_t


def inverse_of(n, p):
    """
    Returns the multiplicative inverse of
    n modulo p.

    This function returns an integer m such that
    (n * m) % p == 1.
    """
    gcd, x, y = extended_euclidean_algorithm(n, p)
    assert (n * x + p * y) % p == gcd

    if gcd != 1:
        # Either n is 0, or p is not a prime number.
        raise ValueError(
            '{} has no multiplicative inverse '
            'modulo {}'.format(n, p))
    else:
        return x % p


class VRF(object):
    def __init__(self):
        self.pk = None
        self.sk = None
        self.log = Logger(self)
        eta.init(rnd.randint(0,369))
        self.g = ecc.gen()
        self.password='somepasskey'
        self.__gen()
        self.order = ecc.order()

    def __gen(self):
        '''
        generate pk/sk
        '''
        # TODO implement that is simple sk choosing mechanism for poc; 
        self.sk = rnd.randint(0,1000)
        self.pk = ecc.scalar_mult(self.sk, self.g)

    def prove(self, x):
        pi = ecc.scalar_mult(inverse_of(x+self.sk, self.order), self.g)
        y = eta.pairing(*self.g[1:], *pi[1:])
        return (y, pi)
    
    '''
    @param y: signed output
    @param pi: [inf, x, y] proof components
    @param pk: [inf, x, y] public key components of the prover 
    '''
    def verify(self, x, y, pi, pk):
        gx = ecc.scalar_mult(x, self.g)
        pk = ecc.scalar_mult(1, pk)
        rhs = eta.pairing(*ecc.scalar_mult(1,self.g)[1:], *pi[1:])
        assert(y == rhs)
        gxs = ecc.add(gx, pk)
        lhs = eta.pairing(*gxs[1:], *pi[1:])
        rhs = eta.pairing(*ecc.scalar_mult(1,self.g)[1:], *ecc.scalar_mult(1,self.g)[1:])
        assert(lhs==rhs)
    

vrf = VRF()
x = 2
y, pi = vrf.prove(x)
vrf.verify(x, y, pi, vrf.pk)