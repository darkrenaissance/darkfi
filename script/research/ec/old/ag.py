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

import numpy as np

add_tuple = lambda a, b: tuple(a_i + b_i for a_i, b_i in zip(a, b))

def shift(a, pos):
    shift_x, shift_y = pos
    c = np.zeros(add_tuple(a.shape, (shift_y, shift_x)), dtype=int)
    c[shift_y:,shift_x:] = a
    return c

def max_shape(shape_a, shape_b):
    a_n, a_m = shape_a
    b_n, b_m = shape_b
    return (max(a_n, b_n), max(a_m, b_m))

def add_shape(shape_a, shape_b):
    a_n, a_m = shape_a
    b_n, b_m = shape_b
    return (a_n + b_n, a_m + b_m)

a = np.array([
    [1, 2, 3],
    [7, 8, 9]
])
print(shift(a, (2, 1)))
