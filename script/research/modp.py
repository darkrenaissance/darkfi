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
print("0x%x" % a.n)
print("\n")
two = modq(2)
inv2 = modq(2).inverse()
print("Inverse of 2 = 0x%x" % inv2.n)
print((two * inv2))
# This is from bellman
inv2_bellman = 0x39f6d3a994cebea4199cec0404d0ec02a9ded2017fff2dff7fffffff80000001
assert inv2.n == inv2_bellman
assert (2 * inv2.n) % q == 1

# Futures contract calculation
multiplier = modq(1)
quantity = modq(100)
entry_price = modq(10000)
exit_price = modq(15000)

initial_margin = multiplier * quantity
print("initial margin =", initial_margin)
price_return = exit_price * entry_price.inverse()
print("R =", price_return)
pnl = initial_margin - (initial_margin * exit_price) * entry_price.inverse()
print("PNL =", pnl)

