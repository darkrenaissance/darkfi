# This module defines multiplication on SharedScalars, using the SPDZ 
# algorithm for multiplication [1].
#
# [1] https://bristolcrypto.blogspot.com/2016/10/what-is-spdz-part-2-circuit-evaluation.html

# Small hack:
# 
# We can't import the SharedScalar class in this module as that would
# create a circular dependency. 
# 
# However, we'd obviously still like to be able to construct new 
# SharedScalars here when doing arithmetic. To be able to do so, 
# we can use `type(sh)` to get access to the SharedScalar class &
# constructor.

from .finite_ring import mod, rand_element
from .secret_sharing import n_to_shares
from random import choice

def mult_2sh(sh1, sh2):
    '''Implements multiplication on two SharedScalars.'''
    # Make sure that these two SharedScalars are compatible 
    sh1._assert_can_operate(sh2)
    
    # Generate a random multiplication triple (public)
    a, b = rand_element(sh1.Q), rand_element(sh1.Q)
    c = mod(a * b, sh1.Q)

    # Share the triple across all machines
    # (It'd be nicer to use the higher-level PrivateScalar.share() here, 
    # but we don't have access to PrivateScalar in this module.)
    machines = list(sh1.owners)
    shared_a = type(sh1)(n_to_shares(a, machines, sh1.Q), sh1.Q)
    shared_b = type(sh1)(n_to_shares(b, machines, sh1.Q), sh1.Q)
    shared_c = type(sh1)(n_to_shares(c, machines, sh1.Q), sh1.Q)

    # Compute sh1 - a, sh2 - b (shared)
    shared_sh1_m_a = sh1 - shared_a
    shared_sh2_m_b = sh2 - shared_b

    # Reconstruct sh1 - a, sh2 - b (public)
    rand_machine = choice(machines)
    sh1_m_a = shared_sh1_m_a.reconstruct(rand_machine).value
    sh2_m_b = shared_sh2_m_b.reconstruct(rand_machine).value

    # Magic! Compute each machine's share of the product
    shared_prod = shared_c + (sh1_m_a * shared_b) + (sh2_m_b * shared_a) + (sh1_m_a * sh2_m_b)
    return shared_prod

def mult_sh_pub(sh, pub):
    '''Implements multiplication on a SharedScalar and a public integer.'''
    # To do the multiplication, we multiply the integer with all shares
    prod_shares = [share * pub for share in sh.shares]
    return type(sh)(prod_shares, Q=sh.Q)