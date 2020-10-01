import random
from polynomial import polynomialsOver
from modp import *



# isIrreducible: Polynomial, int -> bool
# determine if the given monic polynomial with coefficients in Z/p is
# irreducible over Z/p where p is the given integer
# Algorithm 4.69 in the Handbook of Applied Cryptography
def isIrreducible(polynomial, p):
   ZmodP = IntegersModP(p)
   if polynomial.field is not ZmodP:
      raise TypeError("Given a polynomial that's not over %s, but instead %r" %
                        (ZmodP.__name__, polynomial.field.__name__))

   poly = polynomialsOver(ZmodP).factory
   x = poly([0,1])
   powerTerm = x
   isUnit = lambda p: p.degree() == 0

   for _ in range(int(polynomial.degree() / 2)):
      powerTerm = powerTerm.powmod(p, polynomial)
      gcdOverZmodp = gcd(polynomial, powerTerm - x)
      if not isUnit(gcdOverZmodp):
         return False

   return True


# generateIrreduciblePolynomial: int, int -> Polynomial
# generate a random irreducible polynomial of a given degree over Z/p, where p
# is given by the integer 'modulus'. This algorithm is expected to terminate
# after 'degree' many irreducilibity tests. By Chernoff bounds the probability
# it deviates from this by very much is exponentially small.
def generateIrreduciblePolynomial(modulus, degree):
   Zp = IntegersModP(modulus)
   Polynomial = polynomialsOver(Zp)

   while True:
      coefficients = [Zp(random.randint(0, modulus-1)) for _ in range(degree)]
      randomMonicPolynomial = Polynomial(coefficients + [Zp(1)])
      print(randomMonicPolynomial)

      if isIrreducible(randomMonicPolynomial, modulus):
         return randomMonicPolynomial


# create a type constructor for the finite field of order p^m for p prime, m >= 1
@memoize
def FiniteField(p, m, polynomialModulus=None):
   Zp = IntegersModP(p)
   if m == 1:
      return Zp

   Polynomial = polynomialsOver(Zp)
   if polynomialModulus is None:
      polynomialModulus = generateIrreduciblePolynomial(modulus=p, degree=m)

   class Fq(FieldElement):
      fieldSize = int(p ** m)
      primeSubfield = Zp
      idealGenerator = polynomialModulus
      operatorPrecedence = 3

      def __init__(self, poly):
         if type(poly) is Fq:
            self.poly = poly.poly
         elif type(poly) is int or type(poly) is Zp:
            self.poly = Polynomial([Zp(poly)])
         elif isinstance(poly, Polynomial):
            self.poly = poly % polynomialModulus
         else:
            self.poly = Polynomial([Zp(x) for x in poly]) % polynomialModulus

         self.field = Fq

      @typecheck
      def __add__(self, other): return Fq(self.poly + other.poly)
      @typecheck
      def __sub__(self, other): return Fq(self.poly - other.poly)
      @typecheck
      def __mul__(self, other): return Fq(self.poly * other.poly)
      @typecheck
      def __eq__(self, other): return isinstance(other, Fq) and self.poly == other.poly
      @typecheck
      def __ne__(self, other): return not self == other
      
      def __pow__(self, n):
         if n==0: return Fq([1])
         if n==1: return self
         if n%2==0:
            sqrut = self**(n//2)
            return sqrut*sqrut
         if n%2==1: return (self**(n-1))*self
      
      #def __pow__(self, n): return Fq(pow(self.poly, n))
      def __neg__(self): return Fq(-self.poly)
      def __abs__(self): return abs(self.poly)
      def __repr__(self): return repr(self.poly) + ' \u2208 ' + self.__class__.__name__

      @typecheck
      def __divmod__(self, divisor):
         q,r = divmod(self.poly, divisor.poly)
         return (Fq(q), Fq(r))


      def inverse(self):
         if self == Fq(0):
            raise ZeroDivisionError

         x,y,d = extendedEuclideanAlgorithm(self.poly, self.idealGenerator)
         if d.degree() != 0:
            raise Exception('Somehow, this element has no inverse! Maybe intialized with a non-prime?')

         return Fq(x) * Fq(d.coefficients[0].inverse())


   Fq.__name__ = 'F_{%d^%d}' % (p,m)
   return Fq


if __name__ == "__main__":
   F23 = FiniteField(2,3)
   x = F23([1,1])

   F35 = FiniteField(3,5)
   y = F35([1,1,2])
