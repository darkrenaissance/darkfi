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

import matplotlib.pyplot as plt
import numpy as np
import os
import glob

darkies = []
idx = 0
for darkie in glob.glob('log/darkie[0-9]*.log'):
    with open(darkie) as f:
        buf = f.read()
        lines = buf.split('\n')
        apr = float(lines[2].split(':')[1].strip())
        aprs = [float(item) if item != ' ' else 0 for item in lines[3].split(':')[1].split(',')]
        initial_stake = [float(item) for item in lines[0].split(':')[1].split(',')]
        idx +=1
        darkies += [(initial_stake, apr, aprs, idx)]
plt.figure()
# plot initial stake
for darkie in darkies:
    plt.plot(darkie[0])
    plt.title('initial stake')
legends = []
for darkie in darkies:
    legend = ["darkie{}".format(darkie[3])]
    legends +=[legend]
#plt.legend(legends, loc='upper left')
plt.savefig("log/plot_darkies_is.png")
plt.close()

plt.figure()
for darkie in darkies:
    plt.plot(darkie[2])
    plt.title('APR')
plt.savefig('log/plot_darkies_mil_apr.png')
plt.close()
