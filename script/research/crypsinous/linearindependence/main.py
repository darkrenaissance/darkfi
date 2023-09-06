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

import math
import numpy as np
import matplotlib.pyplot as plt
import random

L = 28948022309329048855892746252171976963363056481941560715954676764349967630337

# crypsinous original target function
def target(f, rel_stake):
    T  = L * (1 - (1-f)**rel_stake)
    return T

# naive factorial
def fact(n):
    assert (n>0)
    n = int(n)
    if n==1:
        return 1
    elif n==2:
        return 2
    else:
        return n * fact(n-1)


# all inputs to this function are integers
# sigmas are public
# stake is private
def approx_target_in_zk(sigmas, stake):
    # both sigma_1, sigma_2 are constants, if f is a constant.
    # if f is constant then sigma_12, sigma_2
    # this dictates that tuning need to be hardcoded,
    # secondly the reward, or at least the total stake in the network,
    # can't be anonymous, should be public.
    T = [sigma*stake**(i+1) for i, sigma in enumerate(sigmas)]
    return sum(T)

# approximation of crypsinous targt
def approx_target(c, stake, Sigma, k):
    sigmas = [int((c/Sigma)**i * (L/fact(i))) for i in range(1, k+1)]
    return -1*approx_target_in_zk(sigmas, stake)

def approx_target_with_div(c, stake, Sigma, k):
    sigmas = [(c/Sigma)**i * (L/fact(i)) for i in range(1, k+1)]
    return -1*approx_target_in_zk(sigmas, stake)


f = 0.5
x = (1-f)
c = math.log(x)
# let's assume stakeholde having 1% of the stake, 1/100.
# each iteration increases stake by value 1.
TOTAL = 10000
S = []
stake = 0
targets = []
T = []
T_approx_2term = []
T_approx_3term = []
T_approx_5term = []
k=7
START=1

for i in range(TOTAL):
    if random.random() >= 0.9:
        stake+=1
    S+=[(stake, i+1.0)]
    col = []
    t = target(f, stake/(i+1.0))
    col += [t]
    for j in range(1,k+1):
        t_approx = approx_target_with_div(c, stake, (i+1.0), j)
        col += [t_approx]
    for j in range(1,k+1):
        t_approx = approx_target(c, stake, (i+1.0), j)
        col += [t_approx]
    targets +=[col]

targets = np.array(targets).T

plt.subplot(4,1,1)

plt.plot(targets[0])
for i in range(START,k+1):
    plt.plot(targets[i])

plt.legend(["target"] + ["{} terms".format(i) for i in range(START,k+1)], loc='upper right')

Deltas = []
for j in range(START+1,k+1):
    diff = np.array(targets[j])-np.array(targets[j-1])
    delta = np.sum(diff)
    Deltas += [delta]

print(len(Deltas))
plt.subplot(4,1,2)
Deltas_derivates = np.poly1d(Deltas)
plt.plot(Deltas)
plt.plot(Deltas_derivates.deriv())
plt.legend(["delta", "derivative"], loc='upper right')

plt.subplot(4,1,3)
plt.plot(targets[0])
for i in range(k,2*(k)+1):
    plt.plot(targets[i])

plt.legend(["target"] + ["{} terms(with div)".format(i) for i in range(START,k+1)] , loc='upper right')

Deltas = []
for j in range(k+2,2*(k)+1):
    diff = np.array(targets[j])-np.array(targets[j-1])
    delta = np.sum(diff)
    Deltas += [delta]

plt.subplot(4,1,4)
Deltas_derivates = np.poly1d(Deltas)
plt.plot(Deltas)
plt.plot(Deltas_derivates.deriv())
plt.legend(["delta(with div)", "derivative(with div)"], loc='upper right')


plt.savefig("target.png")
