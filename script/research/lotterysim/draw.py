import matplotlib.pyplot as plt
import numpy as np
import os


LEAD_FILE = "leads.hist"
F_FILE = "f.hist"

LEAD_PROCESSED_IMG = "lead_history_processed.png"
F_PROCESSED_IMG = "f_history_processed.png"

SEP = ","
NODES = 1000 # number of nodes logged

with open(LEAD_FILE) as f:
    buf = f.read()
    nodes = buf.split(SEP)[:-1]
    node_log = []
    for i in range(0, len(nodes)):
        node_log+=[int(nodes[i])]
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
