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

from core.lottery import *
from core.strategy import random_strategy
from core.constants import *
from pid.pid_base import *
from draw import draw
import metrics
import logging
import config
import os

logging.basicConfig(filename='log/vesting.log', encoding='utf-8', level=logging.DEBUG)

def vesting_instance(vesting, running_time):
    os.system("rm log/*_feedback.hist; rm log/*_output.hist log/darkie* log/rewards.log")
    native_drk = ERC20DRK * config.exchange_rate
    total_vesting = 0
    if __name__ == "__main__":
        darkies = []
        id = 0
        for name, distrib in vesting.items():
            darkies += [Darkie(distrib[0] , vesting=distrib, strategy=random_strategy(EPOCH_LENGTH), idx=id)]
            id+=1
            total_vesting+=distrib[-1]
        airdrop = 0
        for darkie in darkies:
            airdrop+=darkie.stake
        print("initial airdrop: {}/{}% on {} nodes".format(airdrop, airdrop/native_drk*100 if native_drk!=0 else 0 , len(darkies)))
        print('total predistribution: {}/{}%'.format(total_vesting, total_vesting/native_drk*100 if native_drk !=0 else 0))
        dt = DarkfiTable(airdrop, running_time, kp=-0.010399999999938556, ki=-0.0365999996461878, kd=0.03840000000000491,  r_kp=-2.53, r_ki=29.5, r_kd=53.77)
        for darkie in darkies:
            dt.add_darkie(darkie)
        dt.background(rand_running_time=False)
        dt.write()

        inflation = metrics.percent_change(airdrop, sum(dt.rewards))
    return (dt.avg_apr(), inflation)

if not os.path.exists(config.vesting_file):
    print('add vested distribution csv at path {} with vesting period {} (slots) aparts.'.format(config.vesting_file, ONE_MONTH))
    exit()

vesting = {}
with open(config.vesting_file) as f:
    for node  in f.readlines():
        keyval = node.split(',')
        key = keyval[0]
        val = ','.join(keyval[1:])
        vesting[keyval[0]] = eval(eval(val))

nodes = len(vesting)
if config.running_time== 0:
    print("Running time is set to 0. Starting sim for whole vesting period...")
    running_time = len(next(iter(vesting.values())))*VESTING_PERIOD
else:
    running_time = config.running_time

apr, inflation = vesting_instance(vesting, running_time)
print('avg apr: {}%'.format(apr*100))
print('inflation: {}%'.format(inflation))
draw()
