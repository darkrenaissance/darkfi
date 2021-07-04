import random
import misc
import pasta
from polynomial_evalrep import make_polynomial_evalrep

n = 8
omega_base = misc.get_omega(pasta.fp, 2**32, seed=0)
assert misc.is_power_of_two(8)
omega = omega_base ** (2 ** 32 // n)
# Order of omega is n
assert omega ** n == 1
# Compute complete roots of this group
ROOTS = [omega ** i for i in range(n)]

PolyEvalRep = make_polynomial_evalrep(pasta.fp, omega, n)

import numpy as np
from tabulate import tabulate

# Wires
a = ["x",  "v1", "v2", "1", "1",  "v3", "e1", "e2"]
b = ["x",  "x",  "x",  "5", "35", "5",  "e3", "e4"]
c = ["v1", "v2", "v3", "5", "35", "35", "e5", "e6"]

wires = a + b + c

# Gates
# La + Rb + Oc + Mab + C = 0
add =           np.array([1, 1, 0, -1,  0])
mul =           np.array([0, 0, 1, -1,  0])
const5 =        np.array([0, 1, 0,  0, -5])
public_input =  np.array([0, 1, 0,  0,  0])
empty =         np.array([0, 0, 0,  0,  0])

gates_matrix = np.array(
    [mul, mul, add, const5, public_input, add, empty, empty])
print("Wires:")
print(tabulate([["a ="] + a, ["b ="] + b, ["c ="] + c]))
print()
print("Gates:")
print(gates_matrix)
print()

# The index of the public input in the gates_matrix
# We specify its position and its value
public_input_values = [(4, 35)]

def permute_indices(wires):
    size = len(wires)
    permutation = [i + 1 for i in range(size)]
    for i in range(size):
        for j in range(i + 1, size):
            if wires[i] == wires[j]:
                permutation[i], permutation[j] = permutation[j], permutation[i]
                break
    return permutation

permutation = permute_indices(wires)

table = [
    ["Wires"] + wires,
    ["Indices"] + list(i + 1 for i in range(len(wires))),
    ["Permutations"] + permutation
]
print(tabulate(table))
print()

import misc
from pasta import fp

def setup(wires, gates_matrix):
    # Section 8.1
    # The selector polynomials that define the circuit's arithmetisation
    gates_matrix = gates_matrix.transpose()
    ql = PolyEvalRep(ROOTS, [fp(i) for i in gates_matrix[0]])
    qr = PolyEvalRep(ROOTS, [fp(i) for i in gates_matrix[1]])
    qm = PolyEvalRep(ROOTS, [fp(i) for i in gates_matrix[2]])
    qo = PolyEvalRep(ROOTS, [fp(i) for i in gates_matrix[3]])
    qc = PolyEvalRep(ROOTS, [fp(i) for i in gates_matrix[4]])
    selector_polys = [ql, qr, qm, qo, qc]

    public_input = [fp(0) for i in range(len(ROOTS))]
    for (index, value) in public_input_values:
        # This is negative because the value is added to
        # the output of the const selector poly:
        # La + Rb + Oc + Mab + (C + PI) = 0
        public_input[index] = fp(-value)
    public_input_poly = PolyEvalRep(ROOTS, public_input)

    # Identity permutations applied to a, b, c
    # Ideally H, k_1 H, k_2 H are distinct cosets of H
    # Here we just sample k and assume it's high-order
    # Random high order k to form distinct cosets
    k = misc.sample_random(fp)
    id_domain_a = ROOTS
    id_domain_b = [k * root for root in ROOTS]
    id_domain_c = [k**2 * root for root in ROOTS]
    id_domain = id_domain_a + id_domain_b + id_domain_c

    # Intermediate step where we permute the positions of the domain
    # generated above
    permuted_domain = [id_domain[i - 1] for i in permutation]
    permuted_domain_a = permuted_domain[:n]
    permuted_domain_b = permuted_domain[n:2 * n]
    permuted_domain_c = permuted_domain[2*n:3 * n]

    # The copy permuation applied to a, b, c
    # Returns the permuted index value (corresponding root of unity coset)
    # when evaluated on the domain.
    ssigma_1 = PolyEvalRep(ROOTS, permuted_domain_a)
    ssigma_2 = PolyEvalRep(ROOTS, permuted_domain_b)
    ssigma_3 = PolyEvalRep(ROOTS, permuted_domain_c)
    copy_permutes = [ssigma_1, ssigma_2, ssigma_3]

setup(wires, gates_matrix)

