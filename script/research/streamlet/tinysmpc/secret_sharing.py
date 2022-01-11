# This module defines how additive secret sharing works in TinySMPC:
#  - how to create secret shares from a number
#  - how to reconstruct the number from the shares
#  - the internal Share class that represents a single secret share
#
# We use the simple additive secret sharing scheme that's compatible
# with SPDZ. This is sort of a well-known "obvious" scheme, so has 
# no canonical citation [1].
#
# However, you can read more about it in [2] and [3].
#
# [1] https://crypto.stackexchange.com/questions/68666/reference-for-additive-secret-sharing
# [2] https://mortendahl.github.io/2017/06/04/secret-sharing-part1/
# [3] https://cs.nyu.edu/courses/spring07/G22.3033-013/scribe/lecture01.pdf

from .fixed_point import fixed_point, float_point
from .finite_ring import assert_is_element, mod, rand_element

class Share():
    '''A class that represents a secret share that belongs to a machine.
       It supports ring arithmetic with other Shares or integers (+, -, *).'''
    def __init__(self, value, owner, Q=None):
        assert_is_element(value, Q)
        self.value = value
        self.owner = owner
        self.Q = Q
        owner.objects.append(self)
        
    def send_to(self, owner):
        '''Send a copy of a Share to a different owner/machine.'''
        return Share(self.value, owner, self.Q)
    
    def __add__(self, other):
        '''Called by: self + other.'''
        self._assert_can_operate(other)
        other_value = other if isinstance(other, int) else other.value 
        sum_value = mod(self.value + other_value, self.Q)
        return Share(sum_value, self.owner, self.Q)
    
    def __radd__(self, other):
        '''Called by: other + self (when other is not a Share).'''
        return self.__add__(other)
    
    def __sub__(self, other):
        '''Called by: self - other.'''
        return self.__add__(-1*other)
    
    def __rsub__(self, other):
        '''Called by: other - self (when other is not a Share).'''
        return (-1*self).__add__(other)
    
    def __mul__(self, other):
        '''Called by: self * other.'''
        self._assert_can_operate(other)
        other_value = other if isinstance(other, int) else other.value
        prod_value = mod(self.value * other_value, self.Q)
        return Share(prod_value, self.owner, self.Q)
    
    def __rmul__(self, other):
        '''Called by: other * self (when other is not a Share).'''
        return self.__mul__(other)

    def __repr__(self):
        return f'Share({self.value}, \'{self.owner.name}\', Q={self.Q})'
    
    def _assert_can_operate(self, other):
        '''Assert that two Shares have the same owners and rings.'''
        if isinstance(other, int): return  # It's okay to do operations with any public integers
        assert self.owner == other.owner, f'{self} and {other} do not have the same owners.'
        assert self.Q == other.Q, f'{self} and {other} are not over the same rings.'

def n_to_shares(n, owners, Q=None):  
    '''Create additive secret Shares for an integer n, split across a group of machines.'''
    # Make sure there are no duplicate owners (technically this is okay, but let's keep it simple)
    assert len(owners) == len(set(owners))

    # Make sure the number actually fits into the finite ring, so we can reconstruct it!
    assert_is_element(n, Q)

    # Generate the value of each secret share using additive secret sharing
    values = [rand_element(Q) for _ in owners[:-1]]
    values.append(mod(n - sum(values), Q))
    
    # Give one secret Share to each machine
    shares = [Share(value, owner, Q) for value, owner in zip(values, owners)]
    
    return shares

def n_from_shares(shares, owner, Q=None):
    '''Given a list of additive secret Shares, reconstruct the integer value they're hiding.'''
    # First, move all shares onto one machine
    local_shares = [share.send_to(owner) for share in shares]
    
    # Now, reconstruct the original value (we just add the shares!)
    return sum(local_shares).value