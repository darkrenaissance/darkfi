load('../mpc/share.sage')

class Source(object):
      def __init__(self, p):
            self.a = random.randint(0,p)
            self.b = random.randint(0,p)
            self.c = self.a*self.b
            self.left_a = random.randint(0,self.a)
            self.right_a = self.a - self.left_a
            self.left_b = random.randint(0,self.b)
            self.right_b = self.b - self.left_b
            self.left_c = random.randint(0,self.c)
            self.right_c = self.c - self.left_c

      def triplet(self, party_id):
            #triple = [self.left_a, self.left_b, self.left_c] if party_id==0 else [self.right_a, self.right_b, self.right_c]
            #return [AuthenticatedShare(share, self, party_id) for share in triple]
            #TODO
            return [AuthenticatedShare(share, self, party_id) for share in [1,1,2]]
