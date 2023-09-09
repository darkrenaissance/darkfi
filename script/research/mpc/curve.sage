# stark curve https://docs.starkware.co/starkex/crypto/stark-curve.html

p = 3618502788666131213697322783095070105623107215331596699973092056135872020481
alpha = 1
# $$y^2 = x^3 + \alpha \dot x + \beta$$  (mod p)
beta = 3141592653589793238462643383279502884197169399375105820974944592307816406665
K = GF(p)
E = EllipticCurve(K, (alpha,beta))
ec_order = E.order()
# ECDSA scheme generator 
G = E(874739451078007766457464989774322083649278607533249481151382481072868806602, 152666792071518830868575557812948353041420400780739481342941381225525861407)

import random
class CurvePoint():
      def __init__(self, x=None, y=None):
          if x==None or y==None:
             self.point = CurvePoint.random()
          else:
                self.point = E(x,y)
          self.x = self.point[0]
          self.y = self.point[1]

      def __repr__(self):
          return "[ x: %s, y: %s, z: 1]"%(self.x, self.y)

      def random(max=p):
          return G * random.randint(0, max)

      def __add__(self, rhs):
          return self.point + rhs.point

      def __sub__(self, rhs):
          return self.point - rhs.point

      def __neg__(self):
          return -1 * self.point
          
      def generator():
          return G

      def __mul__(self, factor):
          return factor * self.point

