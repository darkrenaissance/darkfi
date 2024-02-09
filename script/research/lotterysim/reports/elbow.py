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

import numpy as np
import matplotlib.pyplot as plt
from mpl_toolkits.mplot3d import Axes3D

fig = plt.figure()
ax = fig.add_subplot(projection='3d', azim=30)
with open("gains.txt", "r") as f:
    buf = f.read().split('\n')
    KI = []
    KP = []
    KD = []
    ACC = []
    for line in buf:
        if len(line)<4:
            continue
        cord  = [round(float(i),2) for i in line.split(',')]
        acc = cord[0]
        kp = cord[1]
        ki = cord[2]
        kd = cord[3]
        KI+=[ki]
        KP+=[kp]
        KD+=[kd]
        ACC+=[acc]
img = ax.scatter(KP, KI, KD, c=ACC, cmap=plt.hot())
ax.set_xlabel("KP")
ax.set_ylabel("KI")
ax.set_zlabel("KD")
fig.colorbar(img)
plt.savefig("heuristics.png")
#plt.show()
