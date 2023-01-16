#| # Evaluation Representation of Polynomials and FFT optimizations
#| In addition to the coefficient-based representation of polynomials used
#| in babysnark.py, for performance we will also use an alternative
#| representation where the polynomial is evaluated at a fixed set of points.
#| Some operations, like multiplication and division, are significantly more
#| efficient in this form.
#| We can use FFT-based tools for efficiently converting
#| between coefficient and evaluation representation.
#|
#| This library provides:
#|  - Fast fourier transform for finite fields
#|  - Interpolation and evaluation using FFT

from finite_fields.finitefield import FiniteField
from finite_fields.polynomial import polynomialsOver
from finite_fields.euclidean import extendedEuclideanAlgorithm
import random
from finite_fields.numbertype import typecheck, memoize, DomainElement
from functools import reduce
import numpy as np

#| ## Fast Fourier Transform on Finite Fields
def fft_helper(a, omega, field):
    """
    Given coefficients A of polynomial this method does FFT and returns
    the evaluation of the polynomial at [omega^0, omega^(n-1)]

    If the polynomial is a0*x^0 + a1*x^1 + ... + an*x^n then the coefficients
    list is of the form [a0, a1, ... , an].
    """
    n = len(a)
    assert not (n & (n - 1)), "n must be a power of 2"

    if n == 1:
        return a

    b, c = a[0::2], a[1::2]
    b_bar = fft_helper(b, pow(omega, 2), field)
    c_bar = fft_helper(c, pow(omega, 2), field)
    a_bar = [field(1)] * (n)
    for j in range(n):
        k = j % (n // 2)
        a_bar[j] = b_bar[k] + pow(omega, j) * c_bar[k]
    return a_bar


#| ## Representing a polynomial by evaluation at fixed points
@memoize
def make_polynomial_evalrep(field, omega, n):
    assert n & n - 1 == 0, "n must be a power of 2"

    # Check that omega is an n'th primitive root of unity
    assert type(omega) is field
    omega = field(omega)
    assert omega**(n) == 1
    _powers = [omega**i for i in range(n)]
    assert len(set(_powers)) == n

    _poly_coeff = polynomialsOver(field)

    class PolynomialEvalRep(object):

        def __init__(self, xs, ys):
            # Each element of xs must be a power of omega.
            # There must be a corresponding y for every x.
            if type(xs) is not tuple:
                xs = tuple(xs)
            if type(ys) is not tuple:
                ys = tuple(ys)

            assert len(xs) <= n+1
            assert len(xs) == len(ys)
            for x in xs:
                assert x in _powers
            for y in ys:
                assert type(y) is field

            self.evalmap = dict(zip(xs, ys))

        @classmethod
        def from_coeffs(cls, poly):
            assert type(poly) is _poly_coeff
            assert poly.degree() <= n
            padded_coeffs = poly.coefficients + [field(0)] * (n - len(poly.coefficients))
            ys = fft_helper(padded_coeffs, omega, field)
            xs = [omega**i for i in range(n) if ys[i] != 0]
            ys = [y for y in ys if y != 0]
            return cls(xs, ys)

        def to_coeffs(self):
            # To convert back to the coefficient form, we use polynomial interpolation.
            # The non-zero elements stored in self.evalmap, so we fill in the zero values
            # here.
            ys = [self.evalmap[x] if x in self.evalmap else field(0) for x in _powers]
            coeffs = [b / field(n) for b in fft_helper(ys, 1 / omega, field)]
            return _poly_coeff(coeffs)

        _lagrange_cache = {}
        def __call__(self, x):
            if type(x) is int:
                x = field(x)
            assert type(x) is field
            xs = _powers

            def lagrange(x, xi):
                # Let's cache lagrange values
                if (x,xi) in PolynomialEvalRep._lagrange_cache:
                    return PolynomialEvalRep._lagrange_cache[(x,xi)]

                mul = lambda a,b: a*b
                num = reduce(mul, [x  - xj for xj in xs if xj != xi], field(1))
                den = reduce(mul, [xi - xj for xj in xs if xj != xi], field(1))
                PolynomialEvalRep._lagrange_cache[(x,xi)] = num / den
                return PolynomialEvalRep._lagrange_cache[(x,xi)]

            y = field(0)
            for xi, yi in self.evalmap.items():
                y += yi * lagrange(x, xi)
            return y

        def __mul__(self, other):
            # Scale by integer
            if type(other) is int:
                other = field(other)
            if type(other) is field:
                return PolynomialEvalRep(self.evalmap.keys(),
                                         [other * y for y in self.evalmap.values()])

            # Multiply another polynomial in the same representation
            if type(other) is type(self):
                xs = []
                ys = []
                for x, y in self.evalmap.items():
                    if x in other.evalmap:
                        xs.append(x)
                        ys.append(y * other.evalmap[x])
                return PolynomialEvalRep(xs, ys)

        @typecheck
        def __iadd__(self, other):
            # Add another polynomial to this one in place.
            # This is especially efficient when the other polynomial is sparse,
            # since we only need to add the non-zero elements.
            for x, y in other.evalmap.items():
                if x not in self.evalmap:
                    self.evalmap[x] = y
                else:
                    self.evalmap[x] += y
            return self

        @typecheck
        def __add__(self, other):
            res = PolynomialEvalRep(self.evalmap.keys(), self.evalmap.values())
            res += other
            return res

        def __sub__(self, other): return self + (-other)
        def __neg__(self): return PolynomialEvalRep(self.evalmap.keys(),
                                                    [-y for y in self.evalmap.values()])

        def __truediv__(self, divisor):
            # Scale by integer
            if type(divisor) is int:
                other = field(divisor)
            if type(divisor) is field:
                return self * (1/divisor)
            if type(divisor) is type(self):
                res = PolynomialEvalRep((),())
                for x, y in self.evalmap.items():
                    assert x in divisor.evalmap
                    res.evalmap[x] = y / divisor.evalmap[x]
                return res
            return NotImplemented

        def __copy__(self):
            return PolynomialEvalRep(self.evalmap.keys(), self.evalmap.values())

        def __repr__(self):
            return f'PolyEvalRep[{hex(omega.n)[:15]}...,{n}]({len(self.evalmap)} elements)'

        @classmethod
        def divideWithCoset(cls, p, t, c=field(3)):
            """
              This assumes that p and t are polynomials in coefficient representation,
            and that p is divisible by t.
               This function is useful when t has roots at some or all of the powers of omega,
            in which case we cannot just convert to evalrep and use division above
            (since it would cause a divide by zero.
               Instead, we evaluate p(X) at powers of (c*omega) for some constant cofactor c.
            To do this efficiently, we create new polynomials, pc(X) = p(cX), tc(X) = t(cX),
            and evaluate these at powers of omega. This conversion can be done efficiently
            on the coefficient representation.
               See also: cosetFFT in libsnark / libfqfft.
               https://github.com/scipr-lab/libfqfft/blob/master/libfqfft/evaluation_domain/domains/extended_radix2_domain.tcc
            """
            assert type(p) is _poly_coeff
            assert type(t) is _poly_coeff
            # Compute p(cX), t(cX) by multiplying coefficients
            c_acc = field(1)
            pc = _poly_coeff(list(p.coefficients))  # make a copy
            for i in range(p.degree() + 1):
                pc.coefficients[-i-1] *= c_acc
                c_acc *= c
            c_acc = field(1)
            tc = _poly_coeff(list(t.coefficients))  # make a copy
            for i in range(t.degree() + 1):
                tc.coefficients[-i-1] *= c_acc
                c_acc *= c

            # Divide using evalrep
            pc_rep = cls.from_coeffs(pc)
            tc_rep = cls.from_coeffs(tc)
            hc_rep = pc_rep / tc_rep
            hc = hc_rep.to_coeffs()

            # Compute h(X) from h(cX) by dividing coefficients
            c_acc = field(1)
            h = _poly_coeff(list(hc.coefficients))  # make a copy
            for i in range(hc.degree() + 1):
                h.coefficients[-i-1] /= c_acc
                c_acc *= c

            # Correctness checks
            # assert pc == tc * hc
            # assert p == t * h
            return h


    return PolynomialEvalRep

#| ## Sparse Matrix
#| In our setting, we have O(m*m) elements in the matrix, and expect the number of
#| elements to be O(m).
#| In this setting, it's appropriate to use a rowdict representation - a dense
#| array of dictionaries, one for each row, where the keys of each dictionary
#| are column indices.

class RowDictSparseMatrix():
    # Only a few necessary methods are included here.
    # This could be replaced with a generic sparse matrix class, such as scipy.sparse,
    # but this does not work as well with custom value types like Fp

    def __init__(self, m, n, zero=None):
        self.m = m
        self.n = n
        self.shape = (m,n)
        self.zero = zero
        self.rowdicts = [dict() for _ in range(m)]

    def __setitem__(self, key, v):
        i, j = key
        self.rowdicts[i][j] = v

    def __getitem__(self, key):
        i, j = key
        return self.rowdicts[i][j] if j in self.rowdicts[i] else self.zero

    def items(self):
        for i in range(self.m):
            for j, v in self.rowdicts[i].items():
                yield (i,j), v

    def dot(self, other):
        if isinstance(other, np.ndarray):
            assert other.dtype == 'O'
            assert other.shape in ((self.n,),(self.n,1))
            ret = np.empty((self.m,), dtype='O')
            ret.fill(self.zero)
            for i in range(self.m):
                for j, v in self.rowdicts[i].items():
                    ret[i] += other[j] * v
            return ret

    def to_dense(self):
        mat = np.empty((self.m, self.n), dtype='O')
        mat.fill(self.zero)
        for (i,j), val in self.items():
            mat[i,j] = val
        return mat

    def __repr__(self): return repr(self.rowdicts)

#-
# Examples
if __name__ == '__main__':
    import misc

    Fp = FiniteField(52435875175126190479447740508185965837690552500527637822603658699938581184513,1)  # (# noqa: E501)
    Poly = polynomialsOver(Fp)

    n = 8
    omega = misc.get_omega(Fp, n)
    PolyEvalRep = make_polynomial_evalrep(Fp, omega, n)

    f = Poly([1,2,3,4,5])
    xs = tuple([omega**i for i in range(n)])
    ys = tuple(map(f, xs))
    # print('xs:', xs)
    # print('ys:', ys)

    assert f == PolyEvalRep(xs, ys).to_coeffs()
