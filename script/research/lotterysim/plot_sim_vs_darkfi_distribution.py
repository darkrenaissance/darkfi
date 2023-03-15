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
