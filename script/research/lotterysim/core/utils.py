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
import math
import numpy as np
from core.constants import *

# naive factorial
def fact(n, hp=False):
    assert (n>0)
    n = int(n)
    if n==1:
        return Num(1) if hp else 1
    elif n==2:
        return Num(2) if hp else 2
    else:
        return (Num(n) if hp else n)* fact(n-1, hp)
"""
approximate ouroboros phi function
all inputs to this function are integers
@param sigmas: n sigmas of n-term approximation of phi target function
@param stake: stakeholder stake
@returns: target value T
"""
def approx_target_in_zk(sigmas, stake):
    # both sigma_1, sigma_2 are constants, if f is a constant.
    # if f is constant then sigma_12, sigma_2
    # this dictates that tuning need to be hardcoded,
    # secondly the reward, or at least the total stake in the network,
    # can't be anonymous, should be public.
    T = 0
    for i, sigma in  enumerate(sigmas):
        try:
            T += Num(sigma)*Num(stake)**(i+1)
        except Exception as e:
            T +=0

    return-1*T

def rnd(hp=False):
    return Num(random.random()) if hp else random.random()

def lottery(T, hp=False, log=False):
    y =  rnd(hp) * (L_HP if hp else L)
    if log:
        lottery_line = str(y)+","+str(T)+"\n"
        with open("/tmp/sim_lottery_history.log", "a+") as f:
            f.write(lottery_line)
    won = y < T if y is not None and T is not None else False
    return won, y
