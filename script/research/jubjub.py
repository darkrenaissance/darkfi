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

from finite_fields.modp import IntegersModP

q = 0x73eda753299d7d483339d80809a1d80553bda402fffe5bfeffffffff00000001
modq = IntegersModP(q)

a = modq(-1)
print("a:", hex(a.n))
d = -(modq(10240)/modq(10241))
params = (a, d)

def is_jubjub(params, x, y):
    a, d = params

    return a * x**2 + y**2 == 1 + d * x**2 * y**2

def add(params, point_1, point_2):
    # From here: https://z.cash/technology/jubjub/

    a, d = params

    x1, y1 = point_1
    x2, y2 = point_2

    x3 = (x1 * y2 + y1 * x2) / (1 + d * x1 * x2 * y1 * y2)
    y3 = (y1 * y2 + x1 * x2) / (1 - d * x1 * x2 * y1 * y2)

    return (x3, y3)

def fake_zk_add(params, point_1, point_2):
    # From here: https://z.cash/technology/jubjub/

    a, d = params

    x1, y1 = point_1
    x2, y2 = point_2

    # Compute U = (u1 + v1) * (v2 - EDWARDS_A*u2)
    #           = (u1 + v1) * (u2 + v2)
    U = (x1 + y1) * (x2 + y2)
    assert (x1 + y1) * (x2 + y2) == U

    # Compute A = v2 * u1
    A = y2 * x1
    # Compute B = u2 * v1
    B = x2 * y1
    # Compute C = d*A*B
    C = d * A * B
    assert (d * A) * (B) == C

    # Compute u3 = (A + B) / (1 + C)
    # NOTE: make sure we check for (1 + C) has an inverse
    u3 = (A + B) / (1 + C)
    assert (1 + C) * (u3) == (A + B)

    # Compute v3 = (U - A - B) / (1 - C)
    # We will also need to check inverse here as well.
    v3 = (U - A - B) / (1 - C)
    assert (1 - C) * (v3) == (U - A - B)

    return u3, v3

x = 0x15a36d1f0f390d8852a35a8c1908dd87a361ee3fd48fdf77b9819dc82d90607e
y = 0x015d8c7f5b43fe33f7891142c001d9251f3abeeb98fad3e87b0dc53c4ebf1891

x3, y3 = add(params, (x, y), (x, y))
print(hex(x3.n), hex(y3.n))
u3, v3 = fake_zk_add(params, (x, y), (x, y))
print(hex(u3.n), hex(v3.n))

print(is_jubjub(params, x, y))
print(is_jubjub(params, x3, y3))

print()
print("Identity (0, 1) is jubjub?", is_jubjub(params, 0, 1))
print("Torsion (0, -1) is jubjub?", is_jubjub(params, 0, -1))
double_torsion = add(params, (0, -1), (0, -1))
print("Double torsion is:", hex(double_torsion[0].n), hex(double_torsion[1].n))
dbl_ident = add(params, (0, 1), (0, 1))
print("Double identity is:", hex(dbl_ident[0].n), hex(dbl_ident[1].n))

