# This module defines the conversion functions from float <> int, 
# so that we can use floats in TinySMPC.

from .finite_ring import MAX_INT64, MIN_INT64

PRECISION = 8
MAX_FLOAT = MAX_INT64 / 10**PRECISION  # 92233720368.54776 (floats must be <, not <= this value, due to precision issues)
MIN_FLOAT = MIN_INT64 / 10**PRECISION  # -92233720368.54776 (floats must be >, not >= this value, due to precision issues)

def fixed_point(fl):
    '''Converts a float to an fixed point int, with PRECISION decimal points of precision.'''
    assert MIN_FLOAT < fl < MAX_FLOAT
    return int(fl * 10**PRECISION)

def float_point(n, n_mults=0):
    '''Converts a fixed point integer to a float.
       n_mults is the number of multiplications that generated the int, since multiplications
       of fixed point integers will accumulate extra scaling factors.'''
    scale_factor = (10**PRECISION)**n_mults
    return n / 10**PRECISION / scale_factor