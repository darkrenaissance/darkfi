from logger import Logger
import random as rnd
from  tate_bilinear_pairing import eta, ecc
eta.init(369)

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
        #TODO (res) adhoc temporary
        self.g = ecc.gen()
        self.__gen()
        self.order = ecc.order()

    def __gen(self):
        '''
        generate pk/sk
        '''
        # TODO implement that is simple sk choosing mechanism for poc; 
        self.sk = rnd.randint(0,1000)
        self.pk = ecc.scalar_mult(self.sk, self.g)

    '''
    short signature without random oracle
    @param x: message to be signed
    '''
    def sign(self, x):
        pi = ecc.scalar_mult(inverse_of(x+self.sk, self.order), self.g)
        y = eta.pairing(*self.g[1:], *pi[1:])
        return (y, pi, self.g)
    
    '''
    verify signature
    @param x: signed messaged
    @param y: signature
    @param pi: [inf, x, y] proof components
    @param pk: [inf, x, y] public key components of the prover 
    @param g: group base
    '''
    def verify(x, y, pi, pk_raw, g):
        gx = ecc.scalar_mult(x, g)
        #pk = ecc.scalar_mult(1, pk_raw)
        rhs = eta.pairing(*ecc.scalar_mult(1,g)[1:], *pi[1:])
        if not y == rhs:
            print(f"y: {y}, rhs: {rhs}")
            return False
        gxs = ecc.add(gx, pk_raw)
        lhs = eta.pairing(*gxs[1:], *pi[1:])
        rhs = eta.pairing(*ecc.scalar_mult(1, g)[1:], *ecc.scalar_mult(1, g)[1:])
        if not lhs==rhs:
            print(f"proposed {x}, {y}, {pi}, {pk_raw}, {g}")
            print(f"lhs: {lhs},\nrhs: {rhs}")
            return False
        return True
