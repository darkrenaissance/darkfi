/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

from ouroboros.logger import Logger
import random as rnd
from  tate_bilinear_pairing import eta, ecc
from ouroboros.utils import inverse_of
from ouroboros.utils import vrf_hash
eta.init(369)

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
    
class VRF(object):
    '''
    verifiable random function implementation
    '''
    def __init__(self, seed):
        self.log = Logger(self)
        self.order = ecc.order()
        #TODO use ecc to gen sk
        sk = vrf_hash(seed) % self.order
        g = ecc.gen()
        pk = ecc.scalar_mult(sk, g)
        #
        self.pk = pk 
        self.sk = sk
        self.g=g

    '''
    short signature without random oracle
    @param x: message to be signed
    @return y (the signature), pi (the proof)
    '''
    def sign(self, x):
        pi = ecc.scalar_mult(inverse_of(x+self.sk, self.order), self.g)
        y = eta.pairing(*self.g[1:], *pi[1:])
        self.log.highlight(f"signing slot {x}\nsigma {y}\nproof {pi}\npk {self.pk} \nbase {self.g}")
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
    '''
    def verify(self, x, y, pi):
        gx = ecc.scalar_mult(x, self.g)
        rhs = eta.pairing(*ecc.scalar_mult(1,self.g)[1:], *pi[1:])
        if not y == rhs:
            print(f"y: {y}, rhs: {rhs}")
            return False
        gxs = ecc.add(gx, self.pk)
        lhs = eta.pairing(*gxs[1:], *pi[1:])
        rhs = eta.pairing(*ecc.scalar_mult(1, self.g)[1:], *ecc.scalar_mult(1, self.g)[1:])
        if not lhs==rhs:
            print(f"proposed {x}, {y}, {pi}, {self.pk}, {self.g}")
            print(f"lhs: {lhs},\nrhs: {rhs}")
            return False
        return True
