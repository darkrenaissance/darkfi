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

from lottery import *
from matplotlib import pyplot as plt

TARGET=1
AIRDROP=1000
NODES=5

LOTTERY_FILE = "/tmp/lottery_history.log"
SIM_LOTTERY_FILE = "/tmp/sim_lottery_history.log"

lottery = []

with open(LOTTERY_FILE) as f:
    buf = f.read()
    lines = buf.split("\n")
    RUNNING_TIME = len(lines)
    for line in lines[500:-1]:
        ret = line.split(",")
        lottery +=[[int(ret[0], 16), int(ret[1], 16)]]

if __name__ == "__main__":
    dt  = DarkfiTable(AIRDROP, 0.5, 0.8, 0.8, TARGET, int(RUNNING_TIME/float(NODES)))
    darkies = [Darkie(AIRDROP/float(NODES)) for i in range(NODES)]
    for darkie in darkies:
        dt.add_darkie(darkie)
    dt.background(True, False)
    dt.write()

sim_lottery = []
with open(SIM_LOTTERY_FILE) as f:
    buf = f.read()
    lines = buf.split("\n")
    RUNNING_TIME = len(lines)
    for line in lines[500:-1]:
        ret = line.split(",")
        sim_lottery +=[[float(ret[0]), float(ret[1])]]

plt.scatter([i[0] for i in lottery], [1]*len(lottery), c="#000000")
plt.scatter([i[1] for i in lottery], [3]*len(lottery), c="#ff0000")

plt.scatter([i[0] for i in sim_lottery], [-1]*len(sim_lottery), c="#000000")
plt.scatter([i[1] for i in sim_lottery], [-3]*len(sim_lottery), c="#00ff00")

plt.legend(["darkfid y", "darkfid T", "simulation y", "simulation T"])
plt.savefig("/tmp/lottery_dist.png")
