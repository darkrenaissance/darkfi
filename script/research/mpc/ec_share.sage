def open_2pc(party0_share, party1_share):
    return party0_share + party1_share

def verify_2pc_mac_check(party0_mac, party1_mac):
    assert party0_mac+party1_mac == 0

global_key = random.randint(0, p)

class ECAuthenticatedShare(object):
      """
      additive share
      """
      def __init__(self, share, mac=None, modifier=None):
          self.share = share
          self.mac = global_key * self.share if mac==None else mac
          self.public_modifier = 0  if modifier == None else modifier # carry out extra addition/subtraction by public scalars until opening

      def __repr__(self):
          return "share: %s, mac: %s"%(self.share, self.mac)

      # SPDZ mac authentication
      def authenticated_open(self, peer_authenticated_share):
          opened_share = open_2pc(self.share, peer_authenticated_share.share)
          mac_key = random.randint(0,global_key)
          mac_share = mac_key * (opened_share + self.public_modifier) - self.mac

          peer_mac_key = global_key - mac_key
          peer_mac_share = peer_mac_key * (opened_share + peer_authenticated_share.public_modifier) - peer_authenticated_share.mac
          assert (mac_share + peer_mac_share) == 0

          return opened_share

      def mul_scalar(self, scalar):
          return ECAuthenticatedShare(self.share * scalar, self.mac * scalar, self.public_modifier * scalar)

      def add_point(self, point, party_id):
          return ECAuthenticatedShare(self.share + point, self.mac , self.public_modifier - point) if party_id ==0 else ECAuthenticatedShare(self.share, self.mac, self.public_modifier - point)
          
      def __add__(self, rhs):
          '''
          add additive shares
          '''
          return ECAuthenticatedShare(self.share + rhs.share, self.mac + rhs.mac, self.public_modifier + rhs.public_modifier)

      def __sub__(self, rhs):
          '''
          sub additive shares
          '''
          return ECAuthenticatedShare(self.share - rhs.share, self.mac - rhs.mac, self.public_modifier - rhs.public_modifier)


class ScalingECAuthenticatedShares(object):
      def __init__(self, alpha, beta, triplet, party_id):
          # authenticated shares
          self.alpha_as = alpha
          self.beta_as = beta
          self.a_as = triplet[0]
          self.b_as = triplet[1]
          self.c_as = triplet[2]
          self.party_id = party_id

      def __mul__(self, peer_share):
          generator = CurvePoint.generator()
          
          masked_e_share = self.beta_as - self.b_as
          peer_masked_e_share = peer_share.beta_as - peer_share.b_as
          e = open_2pc(masked_e_share.share, peer_masked_e_share.share)
          peer_masked_d_share = peer_share.alpha_as - peer_share.a_as.mul_point(generator)
          masked_d_share = self.alpha_as - self.a_as.mul_point(generator)
          d = open_2pc(masked_d_share.share, peer_masked_d_share.share)

          return (self.b_as.mul_point(d) + self.a_as.mul_point(generator).mul_scalar(e) + self.c_as.mul_point(generator)).add_point(d * e, self.party_id)

class MSM(object):
      def __init__(self, points,  scalars, source,  party_id):
          '''
          naive multi scalar multiplicatin, between authenticatedpointsshares, and authenticatedscalarshares
          '''
          self.points = points
          self.scalars = scalars
          self.source = source
          self.party_id = party_id

      def msm(self):
          assert (len(self.points) == len(self.scalars))
          beaver = self.source
          point_scalars = []
          for point, scalar in zip(self.points, self.scalars):
              point_scalars += [ScalingECAuthenticatedShares(point, scalar, beaver.triplet(self.party_id), self.party_id)]
          return point_scalars

      def sum(self):
          return sum(self.msm())
