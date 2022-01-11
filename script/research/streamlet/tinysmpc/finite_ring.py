# This module provides useful functions for operating on integers in a finite ring.
#
# (Any integer that is *shared* in TinySMPC must be an element of a finite ring.
#  By default, this is the int64 ring, but we also support modulus prime rings.)

# Mathematical note:
#
# For additive secret sharing to work, we need all of the numbers we're working with
# to be in a finite abelian group under addition. [1] 
# 
# Technically, for SMPC over additive secret sharing, we'd probably like to be able to 
# multiply integers as well, so we're actually operating in a ring.
# 
# This is not a problem, because int64 is a finite ring! [2]
# 
# Another popular choice of a finite abelian ring is the integers modulo a prime [3], 
# with the caveat that this doesn't support negative numbers. Thus, this implementation
# defaults to using the int64 ring. We support prime rings as well, which are explicitly
# used in the PrivateCompare algorithm.
#
# [1] 6.1 in https://cs.nyu.edu/courses/spring07/G22.3033-013/scribe/lecture01.pdf
# [2] https://math.stackexchange.com/q/3692052/28855
# [3] https://mortendahl.github.io/2017/09/03/the-spdz-protocol-part1/

from random import randint, randrange

# Anywhere in the codebase, if Q is None, that means we're computing with int64s!
# This is the default behavior. (See the mathematical note above for why.)
MAX_INT64 =  9223372036854775807
MIN_INT64 = -9223372036854775808

def mod(n, Q=None):
    '''Keeps n inside the finite ring. That is:
         - If we're in a prime ring (Q is the prime size), modulo it by Q
         - If we're in the int64 ring, do the normal int64 overflow behavior
           (we need to explicitly overflow since Python3 ints are unbounded)
    '''
    if Q is not None: return n % Q
    return (n + MAX_INT64 + 1) % 2**64 - (MAX_INT64 + 1)  # https://stackoverflow.com/a/7771499/908744
    
def rand_element(Q=None):
    '''Generates a random int64, or a random integer [0, Q) if Q is specified.
       i.e. an element of the int64 ring, or the size-Q prime ring.'''
    if Q is not None: return randrange(Q)
    return randint(MIN_INT64, MAX_INT64)

def assert_is_element(n, Q=None):
    '''Assert that n is a valid int64, or a valid integer mod Q, if Q is provided.'''
    val = n if isinstance(n, int) else n.value
    if Q is None: 
        assert MIN_INT64 <= val <= MAX_INT64, f'{n} is not an int64 and cannot be reconstructed. Use a smaller value.'
    else:
        assert 0 <= val < Q, f'{n} does not fit inside a size-{Q} prime ring, so it cannot be split into shares that can be reconstructed. Use a larger Q or a smaller value.'
