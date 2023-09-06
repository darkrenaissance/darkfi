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

LEAD_FILE = "/tmp/lead_history.log"
F_FILE = "/tmp/f_history.log"

LEAD_PROCESSED_IMG = "/tmp/lead_history_processed.png"
F_PROCESSED_IMG = "/tmp/f_history_processed.png"

SEP = ","
NODES = 5 # nuber of nodes logged

with open(LEAD_FILE) as f:
    buf = f.read()
    nodes = buf.split(SEP)[:-1]
    node_log = []
    for i in range(0, len(nodes), NODES):
        assert (nodes[i]==nodes[i+1]==nodes[i+2]==nodes[i+3]==nodes[i+4])
        node_log+=[int(nodes[i])]
    freq_single_lead = sum(np.array(node_log)==1)/float(len(node_log))
    print("single leader frequency: {}".format(freq_single_lead))
    plt.plot(node_log)
    plt.savefig(LEAD_PROCESSED_IMG)

with open(F_FILE) as f:
    buf = f.read()
    nodes = buf.split(SEP)[:-1]
    node_log = []
    for i in range(0, len(nodes), NODES):
        assert (nodes[i]==nodes[i+1]==nodes[i+2]==nodes[i+3]==nodes[i+4])
        node_log+=[float(nodes[i])]
    plt.plot(node_log)
    plt.savefig(F_PROCESSED_IMG)
