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
from core.utils import *
from core.strategy import *

NODES=1000

RUNNING_TIME = int(input("running time:"))
if __name__ == "__main__":
    darkies = [Darkie(i, strategy=random_strategy(EPOCH_LENGTH), apy_window=EPOCH_LENGTH) for i in range(NODES)]
    airdrop = 0
    for darkie in darkies:
        airdrop+=darkie.stake
    dt  = DarkfiTable(airdrop, RUNNING_TIME, controller_type=CONTROLLER_TYPE_TAKAHASHI, kc=-2.19, ti=-0.5, td=0.25, ts=-0.35,  r_kp=-0.42, r_ki=2.71, r_kd=-0.239)
    for darkie in darkies:
        dt.add_darkie(darkie)
    acc, apy, reward, stake_ratio, apr = dt.background()
    print('acc: {}'.format(acc))
    dt.write()
