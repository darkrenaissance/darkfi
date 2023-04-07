from core.lottery import *
import os
import numpy
from matplotlib import pyplot as plt

os.system("rm log/f_output.hist; rm log/f_feedback.hist")

RUNNING_TIME = int(input("running time:"))
ERC20DRK=2.1*10**9
NODES=1000
plot = []
EXPS=10
for portion in range(1,11):
    accs = []
    for _ in range(EXPS):
        darkies = []
        egalitarian = ERC20DRK/NODES
        darkies += [ Darkie(random.gauss(egalitarian, egalitarian*0.1), commit=False) for id in range(int(NODES/portion)) ]
        #darkies += [Darkie() for _ in range(NODES*2)]
        airdrop = ERC20DRK
        effective_airdrop  = 0
        for darkie in darkies:
            effective_airdrop+=darkie.stake
        stake_portion = effective_airdrop/airdrop*100
        print("network airdrop: {}, staked token: {}/{}% on {} nodes".format(airdrop, effective_airdrop, stake_portion, len(darkies)))
        dt = DarkfiTable(airdrop, RUNNING_TIME, CONTROLLER_TYPE_DISCRETE, kp=0.005999999999989028, ki=-0.005999999985257798, kd=0.01299999999999478)
        for darkie in darkies:
            dt.add_darkie(darkie)
        acc, apy, reward, staked_ratio, apr = dt.background_with_apy(rand_running_time=False)
        accs += [acc]
    avg_acc = sum(accs)/EXPS*100
    plot+=[(stake_portion, avg_acc)]


plt.plot([x[0] for x in plot], [x[1] for x in plot])
plt.xlabel('drk staked %')
plt.ylabel('accuracy %')
plt.savefig('img'+os.sep+'stake.png')
plt.show()
