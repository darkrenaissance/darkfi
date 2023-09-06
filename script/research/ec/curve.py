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

from finite_fields import finitefield

def add(x_1, y_1, x_2, y_2):
    if (x_1, y_1) == (x_2, y_2):
        if y_1 == 0:
            return None

        # slope of the tangent line
        m = (3 * x_1 * x_1 + a) / (2 * y_1)
        return None
    else:
        if x_1 == x_2:
            return None

        # slope of the secant line
        m = (y_2 - y_1) / (x_2 - x_1)

    x_3 = m*m - x_1 - x_2
    y_3 = m*(x_1 - x_3) - y_1

    return (x_3, y_3)

if __name__ == "__main__":
    # Vesta
    q = 0x40000000000000000000000000000000224698fc0994a8dd8c46eb2100000001
    fq = finitefield.IntegersModP(q)

    a, b = fq(0x00), fq(0x05)

    p = 0x40000000000000000000000000000000224698fc094cf91b992d30ed00000001

    C = (fq(0x1ca18c7c3fcb110f9e92c694ce552238f95e9f9b911599cedaff6018cfc5ed52), fq(0x3ad6133a791e41f3e062d370b40e97e77d20effc00b7ee88c4bb097d245cb438))
    D = (fq(0x3e544e611bb895166afe1a46c6e551c47968daf962d824f79f795cb53585b098), fq(0x2fd03c4da47baf2dfd251e85d18864d4885ddd0e8df648550565b850b79349e3))
    C_plus_D = (fq(0x06f822cbde350215558c46aac9e60eee31afd942ca6da568845ca4f8fe911e17), fq(0x3e294e73970abc197dfff1a14e74cb20c11b81422d9f920c7b0b0c63affdf67b))

    result = add(C[0], C[1], D[0], D[1])
    print(result)
    print(list("%x" % x.n for x in result))
    assert result[0] == C_plus_D[0]
    assert result[1] == C_plus_D[1]
