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

import os
import numpy
from core.strategy import *
from core.lottery import *
import matplotlib.pyplot as plt
import scipy.stats as stats
import math
from draw import draw

os.system("rm log/*_feedback.hist; rm log/*_output.hist")

RUNNING_TIME = int(input("running time:"))
NODES=100

if __name__ == "__main__":
    egalitarian = ERC20DRK/NODES
    darkies = []

    darkie = Darkie(egalitarian, strategy=random_strategy())
    darkies += [darkie]
    darkie = Darkie(int(ERC20DRK-egalitarian), strategy=random_strategy())
    darkies += [darkie]

    airdrop = ERC20DRK
    effective_airdrop  = 0
    for darkie in darkies:
        effective_airdrop+=darkie.stake
    print("network airdrop: {}, staked token: {}/{}% on {} nodes".format(airdrop, effective_airdrop, effective_airdrop/airdrop*100, len(darkies)))
    dt = DarkfiTable(airdrop, RUNNING_TIME, CONTROLLER_TYPE_DISCRETE, kp=-0.010399999999938556, ki=-0.0365999996461878, kd=0.03840000000000491,  r_kp=-2.53, r_ki=29.5, r_kd=53.77)
    for darkie in darkies:
        dt.add_darkie(darkie)
    acc, avg_apy, avg_reward, stake_ratio, avg_apr = dt.background(rand_running_time=False)
    sum_zero_stake = sum([darkie.stake for darkie in darkies[NODES:]])
    print('acc: {}, avg(apr): {}, avg(reward): {}, stake_ratio: {}'.format(acc, avg_apr, avg_reward, stake_ratio))
    print('total stake of 0mint: {}, ratio: {}'.format(sum_zero_stake, sum_zero_stake/ERC20DRK))
    dt.write()
    aprs = []
    fortuners = 0.0
    for darkie in darkies:
        aprs += [float(darkie.apr_scaled_to_runningtime())]
        if darkie.initial_stake[-1] - darkie.initial_stake[0] > 0:
            fortuners+=1

    print('fortuners: {}'.format(str(fortuners/len(darkies))))
    total  = sum([darkie.stake for darkie in darkies])
    for idx, darkie in enumerate(darkies):
        print('{}% idx: {}, stake:{}'.format(float(darkie.stake/total), idx, darkie.stake))
    # distribution of aprs
    aprs = sorted(aprs)
    mu = float(sum(aprs)/len(aprs))
    shifted_aprs = [apr - mu for apr in aprs]
    plt.plot([apr*100 for apr in aprs])
    plt.title('annual percentage return, avg: {:}'.format(mu*100))
    plt.savefig('img/apr_distribution.png')
    plt.show()


    variance = sum(shifted_aprs)/(len(aprs)-1)
    print('mu: {}, variance: {}'.format(str(mu), str(variance)))
    draw()
