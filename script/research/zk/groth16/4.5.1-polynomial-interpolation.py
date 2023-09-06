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

def lagrange(points):
    result = np.poly1d([0])
    for i, (x_i, y_i) in enumerate(points):
        poly = np.poly1d([y_i])
        for j, (x_j, y_j) in enumerate(points):
            if i == j:
                continue
            poly *= np.poly1d([1, -x_j]) / (x_i - x_j)
        #print(poly)
        #print(poly(1), poly(2), poly(3))
        result += poly
    return result

left = lagrange([
    (1, 2), (2, 2), (3, 6)
])
print(left)

right = lagrange([
    (1, 1), (2, 3), (3, 2)
])
print(right)

out = lagrange([
    (1, 2), (2, 6), (3, 12)
])
print(out)

