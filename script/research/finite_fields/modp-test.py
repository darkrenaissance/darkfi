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

from modp import *
from test import test

mod7 = IntegersModP(7)

test(mod7(5), mod7(5)) # Sanity check
test(mod7(5), 1 / mod7(3))
test(mod7(1), mod7(3) * mod7(5))
test(mod7(3), mod7(3) * 1)
test(mod7(2), mod7(5) + mod7(4))

test(True, mod7(0) == mod7(3) + mod7(4))
