load('curve.sage')

class PedersenCommitment(object):
      def __init__(self, value, blinder=None):
          self.value = value
          self.blinder = blinder if blinder is not None else random.randint(0,p)

      def commitment(self):
          return CurvePoint.generator() * self.value + CurvePoint.generator() * self.blinder
