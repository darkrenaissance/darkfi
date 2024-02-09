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

import hashlib
import random

def ff_inv(a, p):
    a %= p

    # extended euclidean algorithm
    # ps + at = 1
    t = 0
    new_t = 1
    r = p
    new_r = a

    while new_r != 0:
        quotient = r // new_r
        t, new_t = new_t, t - quotient * new_t
        r, new_r = new_r, r - quotient * new_r

    assert r == 1
    if t < 0:
        t += p

    return t

class EllipticCurve:

    def __init__(self, p, A, B, order, G, H, J):
        self.p = p
        self.A = A
        self.B = B
        self.order = order
        self.G = G
        self.H = H
        self.J = J
        assert self.is_valid(G)
        assert self.is_valid(H)

    def is_valid(self, P):
        x, y, z = P
        if z == 0:
            return x != 0 or y != 0
        z_inv = ff_inv(z, self.p)
        x, y = x * z_inv, y * z_inv
        return y**2 % self.p == (x**3 + self.A * x + self.B) % self.p

    def add(self, p1, p2):
        x1, y1, z1 = p1
        x2, y2, z2 = p2

        if z1 == 0:
            return (x2, y2, z2)
        elif z2 == 0:
            return (x1, y1, z1)

        if x1 == x2:
            if y1 != y2:
                return (0, 1, 0)

            assert y1 != 0
            m = (3 * x1**2 + self.A) * ff_inv(2*y1, self.p)
        else:
            m = (y2 - y1) * ff_inv(x2 - x1, self.p)

        x3 = (m**2 - x1 - x2) % self.p
        y3 = (m * (x1 - x3) - y1) % self.p
        return (x3, y3, 1)

    def multiply(self, m, p):
        bits = f"{m:b}"
        result = (0, 1, 0)
        temp = p
        for bit in bits[::-1]:
            if bit == "1":
                result = self.add(result, temp)
            temp = self.add(temp, temp)
        return result

    def random_point(self):
        m = self.random_scalar()
        return self.multiply(m, self.G)

    def random_scalar(self):
        m = random.randrange(0, self.order - 1)
        return m

    def random_base(self):
        m = random.randrange(0, self.p - 1)
        return m

def pallas_curve():
    # Pallas
    p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
    q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
    G = (5, 5392431450607408583390510508521091931943415030464003135511088002453056875732, 1)
    H = (9762257241998025279988087154025308614062019274413483967640476725944341089207,
         12058632856930756995627167820351407063813260358041446014729496773111030695755, 1)
    J = (7795559447963065356059848000022900528974048197507738248625163674930282081839,
         5156492880772775379342191094371887365795329446468828588866320184016504353483, 1)
    ec = EllipticCurve(p, 0, 5, q, G, H, J)
    A = (144931808354919915876542440378319484704499556634959420306426167479163065488,
         2699682121356767698440748624399854659825391162912545787181017961871465868196, 1)
    B = (16017037670495191561606513965775243786961447026019262496667491008912834496943,
         20395164507282344548629891414360366999207473153143014512687861307997120664849, 1)
    assert ec.add(A, B) == (2414658659502531855741199170408914396997834981355655923471364687102714431309, 21133344194418979683767005688724798091220515434220043854575260979109407444719, 1)
    m = 26322809409216846271933211244226061368157231119725763192402071651286829040466
    assert ec.multiply(m, G) == (15862887453366837597569434439063150886012590021428640083047997467990450633825, 25887284719793568129480941070850220101898092026705204234126448799557008384178, 1)
    return ec

def pedersen_encrypt(x, y, ec):
    vcv = ec.multiply(x, ec.G)
    vcr = ec.multiply(y, ec.H)
    return ec.add(vcv, vcr)

def _add_to_hasher(hasher, args):
    for arg in args:
        match arg:
            case int() as arg:
                hasher.update(arg.to_bytes(32, byteorder="little"))
            case bytes() as arg:
                hasher.update(arg)
            case list() as arg:
                _add_to_hasher(hasher, arg)
            case _:
                raise Exception(f"unknown hash arg '{arg}' type: {type(arg)}")

def ff_hash(p, *args):
    hasher = hashlib.sha256()
    _add_to_hasher(hasher, args)
    value = int.from_bytes(hasher.digest(), byteorder="little")
    return value % p

def hash_point(point, message=None):
    hasher = hashlib.sha256()
    for x_i in point:
        hasher.update(x_i.to_bytes(32, byteorder="little"))
    # Optional message
    if message is not None:
        hasher.update(message)
    value = int.from_bytes(hasher.digest(), byteorder="little")
    return value

def sign(message, secret, ec):
    ephem_secret = ec.random_scalar()
    ephem_public = ec.multiply(ephem_secret, ec.G)
    challenge = hash_point(ephem_public, message) % ec.order
    response = (ephem_secret + challenge * secret) % ec.order
    return ephem_public, response

def verify(message, signature, public, ec):
    ephem_public, response = signature
    challenge = hash_point(ephem_public, message) % ec.order
    # sG
    lhs = ec.multiply(response, ec.G)
    # R + cP
    rhs_cP = ec.multiply(challenge, public)
    rhs = ec.add(ephem_public, rhs_cP)
    return lhs == rhs

