#!/usr/bin/env python3
import numpy
from finite_fields.finitefield import IntegersModP
from constants import MDS_matrix, round_constants

T = 3
R_F = 8
R_P = 56
RATE = 2

p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001
Fp = IntegersModP(p)

MDS_MATRIX = numpy.array([[Fp(0)] * T] * T)
ROUND_CONSTANTS = []

for i in range(0, T):
    for j in range(0, T):
        MDS_MATRIX[i][j] = Fp(MDS_matrix[i][j])

for i in range(0, R_F + R_P):
    for j in range(0, T):
        ROUND_CONSTANTS.append(Fp(round_constants[i][j]))


def perm(inp):
    half_full_rounds = int(R_F / 2)
    state_words = numpy.array(inp)
    rcf = ROUND_CONSTANTS.copy()

    # First full rounds
    for _ in range(0, half_full_rounds):
        # Round constants, nonlinear layer, matrix multiplication
        for i in range(0, T):
            state_words[i] = state_words[i] + rcf[0]
            rcf.pop(0)
        for i in range(0, T):
            state_words[i] = (state_words[i])**5
        state_words = numpy.array(numpy.dot(MDS_MATRIX, state_words))

    # Middle partial rounds
    for _ in range(0, R_P):
        # Round constants, nonlinear layer, matrix multiplication
        for i in range(0, T):
            state_words[i] = state_words[i] + rcf[0]
            rcf.pop(0)
        state_words[0] = (state_words[0])**5
        state_words = numpy.array(numpy.dot(MDS_MATRIX, state_words))

    # Last full rounds
    for _ in range(0, half_full_rounds):
        # Round constants, nonlinear layer, matrix multiplication
        for i in range(0, T):
            state_words[i] = state_words[i] + rcf[0]
            rcf.pop(0)
        for i in range(0, T):
            state_words[i] = (state_words[i])**5
        state_words = numpy.array(numpy.dot(MDS_MATRIX, state_words))

    return state_words


def debug(w, n, s, m):
    if enable_debug:
        print(f"State {w} {n} absorb:")
        pprint([hex(int(i)) for i in s])
        print(f"Mode {w} {n} absorb:")
        pprint([hex(int(i)) if i is not None else None for i in m])


def poseidon_hash(messages):
    L = len(messages)
    k = int((L + RATE - 1) / RATE)
    padding = [Fp(0)] * (k * RATE - L)
    messages.extend(padding)

    # Sponge
    mode = [None] * RATE
    output = [None] * RATE
    state = [Fp(0)] * T

    initial_capacity_element = Fp(L << 64)
    state[RATE] = initial_capacity_element

    # absorb sponge
    for n, value in enumerate(messages):
        debug("before", n + 1, state, mode)
        loop = False  # Use this to mark we should reiterate
        for i in range(0, len(mode)):
            if mode[i] is None:
                mode[i] = value
                loop = True
                break

        if loop:
            debug("after", n + 1, state, mode)
            continue

        # zip short-circuits when one iterator completes, so this will
        # only mutate the rate portion of the state.
        for i, _ in enumerate(zip(state, mode)):
            state[i] += mode[i]

        state = perm(state)

        output = [None] * RATE
        for i, _ in enumerate(zip(output, state)):
            output[i] = state[i]

        mode = [None] * RATE
        mode[0] = value

        debug("after", n + 1, state, mode)

    debug("before", "final", state, mode)
    for i, _ in enumerate(zip(state, mode)):
        state[i] += mode[i]

    state = perm(state)

    output = [None] * RATE
    for i, _ in enumerate(zip(output, state)):
        output[i] = state[i]

    mode = output
    debug("after", "final", state, mode)
    return output[0]


if __name__ == "__main__":
    enable_debug = False
    if enable_debug:
        from pprint import pprint

    #input_words = []
    #for i in range(0, T):
    #    input_words.append(Fp(i))
    #output_words = perm(input_words)
    #print([hex(int(i)) for i in output_words])

    words = [Fp(1), Fp(2)]
    #words = [Fp(1), Fp(2), Fp(3), Fp(4), Fp(5)]
    h = poseidon_hash(words)
    print(hex(int(h)))
