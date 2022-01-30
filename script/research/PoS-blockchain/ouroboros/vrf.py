from streamlet.logger import Logger
import random as rnd
from  tate_bilinear_pairing import eta, ecc
from ouroboros.utils import inverse_of
eta.init(369)

class VRF(object):
    '''
    verifiable random function implementation
    '''
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
