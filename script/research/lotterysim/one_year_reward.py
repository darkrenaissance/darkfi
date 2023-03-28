from lottery import *
import os
import numpy
from matplotlib import pyplot as plt

os.system("rm f.hist; rm leads.hist")

RUNNING_TIME = 100000
NODES=1000
NORM_NODES = NODES/10

stakes =  [100, 1000, 10000, 100000]
rewards = []
airdrop = ERC20DRK

for stake in stakes:
    effective_airdrop  = 0
    darkies = []
    norm_staker_sum = stake*NORM_NODES
    egalitarian = (ERC20DRK-norm_staker_sum)/NODES
    darkies += [ Darkie(random.gauss(egalitarian, egalitarian*0.1)) for id in range(int(NODES)) ]
    darkies += [Darkie(stake) for _ in range(int(NORM_NODES))]
    for darkie in darkies:
        effective_airdrop+=darkie.stake
    dt = DarkfiTable(effective_airdrop, RUNNING_TIME, CONTROLLER_TYPE_DISCRETE, kp=-0.010399999999938556, ki=-0.0365999996461878, kd=0.03840000000000491)
    for darkie in darkies:
        dt.add_darkie(darkie)
    acc = dt.background(rand_running_time=False)
    sum_zero_stake = sum([darkie.stake for darkie in darkies[NODES:]])
    avg_zero_stake = sum_zero_stake/NORM_NODES
    reward = ((avg_zero_stake/stake)-1)
    print("stake: {}, acc: {}, reward: {}%".format(stake, acc*100, reward*100))
    rewards += [(stake, reward)]
print('avg rwards: {}%'. format(sum([r[1] for r in rewards])/len(stakes)))

plt.plot([r[0] for r in rewards], [r[1] for r in rewards])
plt.show()
