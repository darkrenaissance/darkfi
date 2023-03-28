# running time ONE_YEAR
#network airdrop: 2100000000.0, staked token: 2099223067.1631408/99.9630031982448% on 1000 nodes
#acc: 37.5249500998004, reward: 0.0005964171059733166794378382704% with instant finality: 0.001589388138790594006649208260%

from lottery import *
import os
import numpy

os.system("rm f.hist; rm leads.hist")

RUNNING_TIME = ONE_YEAR
NODES=1000

rewards = []
airdrop = ERC20DRK
darkies = []


effective_airdrop  = 0
egalitarian = ERC20DRK/NODES
darkies = [ Darkie(random.gauss(egalitarian, egalitarian*0.01)) for id in range(int(NODES)) ]
for darkie in darkies:
    effective_airdrop+=darkie.stake
print("network airdrop: {}, staked token: {}/{}% on {} nodes".format(airdrop, effective_airdrop, effective_airdrop/airdrop*100, len(darkies)))
dt = DarkfiTable(effective_airdrop, RUNNING_TIME, CONTROLLER_TYPE_DISCRETE, kp=-0.010399999999938556, ki=-0.0365999996461878, kd=0.03840000000000491)
for darkie in darkies:
    dt.add_darkie(darkie)
acc = dt.background(rand_running_time=False)
apy_per = sum([darkie.apy_percentage() for darkie in darkies])/NODES
print("acc: {}, reward: {}% with instant finality: {}%".format(acc*100, apy_per, apy_per/Num(acc)))
