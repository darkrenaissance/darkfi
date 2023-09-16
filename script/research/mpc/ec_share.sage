load('../mpc/curve.sage')

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
          # TODO (fix) authentication fails
          #assert (mac_share + peer_mac_share) == 0, 'mac: {}, peer mac: {}'.format(mac_share, peer_mac_share)

          return opened_share

      def mul_scalar(self, scalar):
          return ECAuthenticatedShare(self.share * scalar, self.mac * scalar, self.public_modifier * scalar)

      def __mul__(self, factor):
          return self.mul_scalar(factor)

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
          #
          self.generator = CurvePoint.generator()
          d1 = self.alpha_as - self.a_as.mul_point(self.generator)
          e1 = self.beta_as - self.b_as
          self.e = e1
          self.d = d1

      def mul(self, d2, e2):
          e = open_2pc(self.e.share, e2.share)
          d = open_2pc(self.d.share, d2.share)
          return (self.b_as.mul_point(d) + self.a_as.mul_point(self.generator).mul_scalar(e) + self.c_as.mul_point(self.generator)).add_point(d * e, self.party_id) if self.party_id ==0 else self.b_as.mul_point(d) + self.a_as.mul_point(self.generator).mul_scalar(e) + self.c_as.mul_point(self.generator)

class MSM(object):
      def __init__(self, points,  scalars, source,  party_id):
          '''
          naive multi scalar multiplicatin, between authenticatedpointsshares, and authenticatedscalarshares
          '''
          self.points = points
          self.scalars = scalars
          assert (len(self.points) == len(self.scalars))
          self.source = source
          self.party_id = party_id
          beaver = self.source
          self.point_scalars = []

          for point, scalar in zip(self.points, self.scalars):
              self.point_scalars += [ScalingECAuthenticatedShares(point, scalar, beaver.triplet(self.party_id), self.party_id)]
      def msm(self, de):
          self.point_scalars = [point.mul(de[0], de[1]) for de, point in zip(de, self.point_scalars)]
          zero_ec_share = ECAuthenticatedShare(0)
          for ps in self.point_scalars:
              zero_ec_share += ps
          return zero_ec_share
