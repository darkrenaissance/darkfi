# This module defines addition on SharedScalars, using the SPDZ algorithm 
# for addition [1].
#
# Technically, this method is extremely simple as it follows directly 
# from additive secret sharing, and likely predates SPDZ.
#
# [1] "Computations" on pg 6 of https://eprint.iacr.org/2011/535.pdf

# Small hack:
# 
# We can't import the SharedScalar class in this module as that would
# create a circular dependency. 
# 
# However, we'd obviously still like to be able to construct new 
# SharedScalars here when doing arithmetic. To be able to do so, 
# we can use `type(sh)` to get access to the SharedScalar class &
# constructor.

def add_2sh(sh1, sh2):
    '''Implements addition on two SharedScalars.'''
    # To do the addition, we add each machine's shares together
    sh1._assert_can_operate(sh2)
    sum_shares = [sh1.share_of[owner] + sh2.share_of[owner]
                  for owner in sh1.owners]
    return type(sh1)(sum_shares, Q=sh1.Q)

def add_sh_pub(sh, pub):
    '''Implements addition on a SharedScalar and a public integer.'''
    # To do the addition, we add the integer to one (random) share only
    new_shares = [sh.shares[0] + pub] + sh.shares[1:]
    return type(sh)(new_shares, sh.Q)
