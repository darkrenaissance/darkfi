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

import matplotlib.pyplot as plt

a = []
for i in range(-500, 500, 1):
    row = []
    for j in range(-500, 500, 1):
        x, y = float(j), float(i)
        x /= 2000
        y /= 2000
        v = y**2 - x**3 - 5
        #if 0.98 < x < 1.02 and 2.4 < y < 2.6:
        #    print(v)
        v = int(v * 1000)
        row.append(v)
    a.append(row)
plt.imshow(a, cmap='hot', interpolation='nearest')
plt.show()
