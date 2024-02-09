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

LEAD_FILE = 'log'+os.sep+"f_feedback.hist"
F_FILE = 'log'+os.sep+"f_output.hist"

LEAD_PROCESSED_IMG = 'img'+os.sep+"feedback_history_processed.png"
F_PROCESSED_IMG = 'img'+os.sep+"output_history_processed.png"

SEP = ","
NODES = 1000 # number of nodes logged

def draw():
    with open(LEAD_FILE) as f:
        buf = f.read()
        nodes = buf.split(SEP)[:-1]
        node_log = []
        for i in range(0, len(nodes)):
            node_log+=[int(float(nodes[i]))]
        freq_single_lead = sum(np.array(node_log)==1)/float(len(node_log))
        print("single leader frequency: {}".format(freq_single_lead))
        plt.plot(node_log)
        plt.legend(['#leads'])
        plt.savefig(LEAD_PROCESSED_IMG)


    with open(F_FILE) as f:
        buf = f.read()
        nodes = buf.split(SEP)[:-1]
        node_log = []
        for i in range(0, len(nodes)):
            node_log+=[float(nodes[i])]
        plt.plot(node_log)
        plt.legend(['#leads', 'f'])
        plt.savefig(F_PROCESSED_IMG)
