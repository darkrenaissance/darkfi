/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

#!/usr/bin/env python3
import numpy
from finite_fields.finitefield import IntegersModP
from constants import round_constants, MDS_matrix

# Width
T = 3
# Full rounds
R_F = 8
# Partial rounds
R_P = 56
# Sponge rate
RATE = 2

# pallas
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
            state_words[i] = (state_words[i])**5  # sbox
        state_words = numpy.array(numpy.dot(MDS_MATRIX, state_words))

    # Middle partial rounds
    for _ in range(0, R_P):
        # Round constants, nonlinear layer, matrix multiplication
        for i in range(0, T):
            state_words[i] = state_words[i] + rcf[0]
            rcf.pop(0)
        state_words[0] = (state_words[0])**5  # sbox
        state_words = numpy.array(numpy.dot(MDS_MATRIX, state_words))

    # Last full rounds
    for _ in range(0, half_full_rounds):
        # Round constants, nonlinear layer, matrix multiplication
        for i in range(0, T):
            state_words[i] = state_words[i] + rcf[0]
            rcf.pop(0)
        for i in range(0, T):
            state_words[i] = (state_words[i])**5  # sbox
        state_words = numpy.array(numpy.dot(MDS_MATRIX, state_words))

    return state_words


def poseidon_hash(messages):
    L = len(messages)
    k = int((L + RATE - 1) / RATE)
    padding = [Fp(0)] * (k * RATE - L)
    messages.extend(padding)

    # Sponge
    mode = [None] * RATE
    output = [None] * RATE
    state = [Fp(0)] * T

    # Capacity value is L ⋅ 2^64 + (o-1) where o is the output length
    initial_capacity_element = Fp(L << 64)
    state[RATE] = initial_capacity_element

    # This outermost loop absorbs the messages in the sponge.
    for n, value in enumerate(messages):
        loop = False  # Use this to mark we should reiterate
        for i in range(0, len(mode)):
            if mode[i] is None:
                mode[i] = value
                loop = True
                break

        if loop:
            continue

        # zip short-circuits when one iterator completes, so this will
        # only mutate the rate portion of the state.
        for i, _ in enumerate(zip(state, mode)):
            state[i] += mode[i]

        # Permutation of the current state
        state = perm(state)

        for i, _ in enumerate(zip(output, state)):
            output[i] = state[i]

        # Reinit sponge with the current message as the first value.
        mode = [None] * RATE
        mode[0] = value

    for i, _ in enumerate(zip(state, mode)):
        state[i] += mode[i]

    # Permutation of the final state
    state = perm(state)

    for i, _ in enumerate(zip(output, state)):
        output[i] = state[i]

    # Sponge now has the output, so the first element is our hash.
    mode = output
    return output[0]


if __name__ == "__main__":
    words = []
    for word in range(0, 10):
        words.append(Fp(word))
        h = poseidon_hash(words.copy())
        print(hex(int(h)))
