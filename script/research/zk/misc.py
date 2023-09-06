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

import random

def sample_random(fp, seed=None):
    rnd = random.Random(seed)
    # Range of the field is 0 ... p - 1
    return fp(rnd.randint(0, fp.p - 1))

def is_power_of_two(n):
    # Power of two number is represented by a single digit
    # followed by zeroes.
    return n & (n - 1) == 0

#| ## Choosing roots of unity
def get_omega(fp, n, seed=None):
    """
    Given a field, this method returns an n^th root of unity.
    If the seed is not None then this method will return the
    same n'th root of unity for every run with the same seed

    This only makes sense if n is a power of 2.
    """
    assert is_power_of_two(n)
    # https://crypto.stackexchange.com/questions/63614/finding-the-n-th-root-of-unity-in-a-finite-field
    while True:
        # Sample random x != 0
        x = sample_random(fp, seed)
        # Compute g = x^{(q - 1)/n}
        y = pow(x, (fp.p - 1) // n)
        # If g^{n/2} != 1 then g is a primitive root
        if y != 1 and pow(y, n // 2) != 1:
            assert pow(y, n) == 1, "omega must be 2nd root of unity"
            return y

