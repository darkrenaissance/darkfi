# memoize calls to the class constructors for fields
# this helps typechecking by never creating two separate
# instances of a number class.
def memoize(f):
   cache = {}

   def memoizedFunction(*args, **kwargs):
      argTuple = args + tuple(kwargs)
      if argTuple not in cache:
         cache[argTuple] = f(*args, **kwargs)
      return cache[argTuple]

   memoizedFunction.cache = cache
   return memoizedFunction


# type check a binary operation, and silently typecast 0 or 1
def typecheck(f):
   def newF(self, other):
      if (hasattr(other.__class__, 'operatorPrecedence') and
            other.__class__.operatorPrecedence > self.__class__.operatorPrecedence):
         return NotImplemented

      if type(self) is not type(other):
         try:
            other = self.__class__(other)
         except TypeError:
            message = 'Not able to typecast %s of type %s to type %s in function %s'
            raise TypeError(message % (other, type(other).__name__, type(self).__name__, f.__name__))
         except Exception as e:
            message = 'Type error on arguments %r, %r for functon %s. Reason:%s'
            raise TypeError(message % (self, other, f.__name__, type(other).__name__, type(self).__name__, e))

      return f(self, other)

   return newF



# require a subclass to implement +-* neg and to perform typechecks on all of
# the binary operations finally, the __init__ must operate when given a single
# argument, provided that argument is the int zero or one
class DomainElement(object):
   operatorPrecedence = 1

   # the 'r'-operators are only used when typecasting ints
   def __radd__(self, other): return self + other
   def __rsub__(self, other): return -self + other
   def __rmul__(self, other): return self * other

   # square-and-multiply algorithm for fast exponentiation
   def __pow__(self, n):
      if type(n) is not int:
         raise TypeError

      Q = self
      R = self if n & 1 else self.__class__(1)

      i = 2
      while i <= n:
         Q = (Q * Q)

         if n & i == i:
            R = (Q * R)

         i = i << 1

      return R


   # requires the additional % operator (i.e. a Euclidean Domain)
   def powmod(self, n, modulus):
      if type(n) is not int:
         raise TypeError

      Q = self
      R = self if n & 1 else self.__class__(1)

      i = 2
      while i <= n:
         Q = (Q * Q) % modulus

         if n & i == i:
            R = (Q * R) % modulus

         i = i << 1

      return R



# additionally require inverse() on subclasses
class FieldElement(DomainElement):
   def __truediv__(self, other): return self * other.inverse()
   def __rtruediv__(self, other): return self.inverse() * other
   def __div__(self, other): return self.__truediv__(other)
   def __rdiv__(self, other): return self.__rtruediv__(other)

