# Notes from paper:
# "Efficient Zero-Knowledge Arguments for Arithmetic Circuits in the
#  Discrete Log Setting" by Bootle and others (EUROCRYPT 2016)

from finite_fields import finitefield
import numpy as np

q = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
fq = finitefield.IntegersModP(q)

# Number of variables
m = 16
# Number of rows for multiplication statements
n = 2

N = n * m

# Initialize zeroed table
aux = np.full(m, fq(0))

# From the zk-explainer document, we will represent the function:
#
#    def foo(w, a, b):
#        if w:
#            return a * b
#        else:
#            return a + b
#
# Which can be translated mathematically to the statements:
#
#   ab = m
#   w(m - a - b) = v - a - b
#   w^2 = w
#
# Where m is an intermediate value.

one = 0
aux[one] = fq(1)

a = 1
b = 2
w = 3

aux[a] = fq(110)
aux[b] = fq(4)
aux[w] = fq(1)

# Calculate intermediate advice values
m = 4
aux[m] = aux[a] * aux[b]

# Calculate public input values
v = 5
aux[v] = aux[w] * (aux[a] * aux[b]) + \
    (aux[one] - aux[w]) * (aux[a] + aux[b])

# Just a quick enforcement check:
assert aux[a] * aux[b] == aux[m]
assert aux[w] * (aux[m] - aux[a] - aux[b]) == aux[v] - aux[a] - aux[b]
assert aux[w] * aux[w] == aux[w]

# Setup the gates. For each row of a, b and c, the statement a b = c holds
