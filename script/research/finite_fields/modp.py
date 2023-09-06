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


from .euclidean import *
from .numbertype import *

# so all IntegersModP are instances of the same base class
class _Modular(FieldElement):
    pass


@memoize
def IntegersModP(p):
    # assume p is prime

    class IntegerModP(_Modular):
        def __init__(self, n):
            try:
                self.n = int(n) % IntegerModP.p
            except:
                raise TypeError("Can't cast type %s to %s in __init__" %
                    (type(n).__name__, type(self).__name__))

            self.field = IntegerModP

        @typecheck
        def __add__(self, other):
            return IntegerModP(self.n + other.n)

        @typecheck
        def __sub__(self, other):
            return IntegerModP(self.n - other.n)

        @typecheck
        def __mul__(self, other):
            return IntegerModP(self.n * other.n)

        def __neg__(self):
            return IntegerModP(-self.n)

        @typecheck
        def __eq__(self, other):
            return isinstance(other, IntegerModP) and self.n == other.n

        @typecheck
        def __ne__(self, other):
            return isinstance(other, IntegerModP) is False or self.n != other.n

        @typecheck
        def __divmod__(self, divisor):
            q,r = divmod(self.n, divisor.n)
            return (IntegerModP(q), IntegerModP(r))

        def inverse(self):
            # need to use the division algorithm *as integers* because we're
            # doing it on the modulus itself (which would otherwise be zero)
            x,y,d = extendedEuclideanAlgorithm(self.n, self.p)

            if d != 1:
                raise Exception("Error: p is not prime in %s!" % (self.__name__))

            return IntegerModP(x)

        def __abs__(self):
            return abs(self.n)

        def __str__(self):
            return str(self.n)

        def __repr__(self):
            return '%d (mod %d)' % (self.n, self.p)

        def __int__(self):
            return self.n

        def __hash__(self):
            return hash((self.n, self.p))

    IntegerModP.p = p
    IntegerModP.__name__ = 'Z/%d' % (p)
    IntegerModP.englishName = 'IntegersMod%d' % (p)
    return IntegerModP


if __name__ == "__main__":
    mod7 = IntegersModP(7)
