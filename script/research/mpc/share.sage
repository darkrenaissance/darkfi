import random
load('../mpc/curve.sage')
load('../mpc/ec_share.sage')

def open_2pc(party0_share, party1_share):
    return party0_share + party1_share

def verify_2pc_mac_check(party0_mac, party1_mac):
    assert party0_mac+party1_mac == 0

global_key = random.randint(0, p)

class AuthenticatedShare(object):
      """
      additive share
      """
      def __init__(self, share, source, party_id, mac=None, modifier=None):
          self.share = share
          self.mac = global_key * self.share if mac==None else mac
          self.public_modifier = 0  if modifier == None else modifier # carry out extra addition/subtraction by public scalars until opening
          self.party_id = party_id
          self.source = source


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

      def sub_scalar(self, scalar, party_id):
          return AuthenticatedShare(self.share - scalar, self.mac, self.public_modifier + scalar) if party_id == 0 else AuthenticatedShare(self.share , self.mac, self.public_modifier + scalar)

      def add_scalar(self, scalar, party_id):
          return AuthenticatedShare(self.share + scalar, self.mac , self.public_modifier - scalar) if party_id ==0 else AuthenticatedShare(self.share, self.mac, self.public_modifier - scalar)

      def mul_scalar(self, scalar):
          return AuthenticatedShare(self.share * scalar, self.mac * scalar, self.public_modifier * scalar)

      def mul_point(self, point):
          return ECAuthenticatedShare(self.share * point, self.mac * point, self.public_modifier * point)

      def __add__(self, rhs):
          '''
          add additive shares
          '''
          return AuthenticatedShare(self.share + rhs.share, self.mac + rhs.mac, self.public_modifier + rhs.public_modifier)

      def __sub__(self, rhs):
          '''
          sub additive shares
          '''
          return AuthenticatedShare(self.share - rhs.share, self.mac - rhs.mac, self.public_modifier - rhs.public_modifier)

'''
class MultiplicationAuthenticatedShares(object):
      def __init__(self, alpha, beta, triplet, party_id):
          # authenticated shares
          self.alpha_as = alpha
          self.beta_as = beta
          self.a_as = triplet[0]
          self.b_as = triplet[1]
          self.c_as = triplet[2]
          self.party_id = party_id

      def __mul__(self, peer_share):
          masked_d_share = self.alpha_as - self.a_as
          peer_masked_d_share = peer_share.alpha_as - peer_share.a_as
          d = open_2pc(masked_d_share.share, peer_masked_d_share.share)

          masked_e_share = self.beta_as - self.b_as
          peer_masked_e_share = peer_share.beta_as - peer_share.b_as
          e = open_2pc(masked_e_share.share, peer_masked_e_share.share)

          return (self.b_as.mul_scalar(d) + self.a_as.mul_scalar(e) + self.c_as).add_scalar(d*e, self.party_id)
'''

class MultiplicationAuthenticatedShares(object):
      def __init__(self, alpha, beta, triplet, party_id):
          # authenticated shares
          self.alpha_as = alpha
          self.beta_as = beta
          self.a_as = triplet[0]
          self.b_as = triplet[1]
          self.c_as = triplet[2]
          self.party_id = party_id

          d1 = self.alpha_as - self.a_as
          e1 = self.beta_as - self.b_as
          print('[{}] beta: {}, b: {}'.format(self.party_id, self.beta_as, self.b_as))
          print('[{}] e: {}'.format(self.party_id, e1))
          self.d = d1
          self.e = e1


      def mul(self, d2, e2):
          d = open_2pc(self.d.share, d2.share)
          e = open_2pc(self.e.share, e2.share)
          if self.party_id==0:
              bd = self.b_as.mul_scalar(d)
              ae = self.a_as.mul_scalar(e)
              return (bd + ae + self.c_as).add_scalar(d*e, self.party_id)
          else:
              bd = self.b_as.mul_scalar(d)
              ae = self.a_as.mul_scalar(e)
              #return (bd + ae + self.c_as).add_scalar(d*e, self.party_id)
              return  bd + ae + self.c_as
