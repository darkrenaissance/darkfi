from streamlet.logger import Logger
import random as rnd
from  tate_bilinear_pairing import eta, ecc
from ouroboros.utils import inverse_of
from utils import vrf_hash
eta.init(369)

'''
gernate vrf keys for stakeholder
@param sk_seed: this is suppoed to be the password of the stakeholder
'''
def generate_vrf_keys(sk_seed):
    '''
        generate pk/sk
    return: list of pk (public key), sk(secret key), base(field base)
    '''
    sk = vrf_hash(sk_seed)
    base = ecc.gen()
    pk = ecc.scalar_mult(sk, base)
    return (pk, sk, base)

class VRF(object):
    '''
    verifiable random function implementation
    '''
    def __init__(self, pk, sk, base):
        self.pk = pk
        self.sk = sk
        self.g=base
        self.log = Logger(self)
        self.order = ecc.order()

    '''
    short signature without random oracle
    @param x: message to be signed
    @return y (the signature), pi (the proof)
    '''
    def sign(self, x):
        pi = ecc.scalar_mult(inverse_of(x+self.sk, self.order), self.g)
        y = eta.pairing(*self.g[1:], *pi[1:])
        return (y, pi)

    def update(self, pk, sk, g):
        self.pk = pk
        self.sk = sk
        self.g = g
    
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