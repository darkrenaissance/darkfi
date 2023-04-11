import os
import numpy
from core.strategy import *
from core.lottery import *

os.system("rm log/*_feedback.hist; rm log/*_output.hist")

RUNNING_TIME = int(input("running time:"))

NODES=100

if __name__ == "__main__":
    egalitarian = ERC20DRK/NODES
    darkies = []
    for id in range(int(NODES)):
      darkie = Darkie(random.gauss(egalitarian, egalitarian*0.1), strategy=random_strategy(EPOCH_LENGTH))
      darkies += [darkie]

    #TODO try rpid with 0mint
    #darkies += [Darkie(0, strategy=LinearStrategy(EPOCH_LENGTH)) for _ in range(NODES)]
    airdrop = ERC20DRK
    effective_airdrop  = 0
    for darkie in darkies:
        effective_airdrop+=darkie.stake
    print("network airdrop: {}, staked token: {}/{}% on {} nodes".format(airdrop, effective_airdrop, effective_airdrop/airdrop*100, len(darkies)))
    dt = DarkfiTable(airdrop, RUNNING_TIME, CONTROLLER_TYPE_DISCRETE, kp=-0.010399999999938556, ki=-0.0365999996461878, kd=0.03840000000000491,  r_kp=0.17, r_ki=-0.09, r_kd=1.1)
    for darkie in darkies:
        dt.add_darkie(darkie)
    acc, avg_apy, avg_reward, stake_ratio, avg_apr = dt.background(rand_running_time=False)
    sum_zero_stake = sum([darkie.stake for darkie in darkies[NODES:]])
    print('acc: {}, avg(apr): {}, avg(reward): {}, stake_ratio: {}'.format(acc, avg_apr, avg_reward, stake_ratio))
    print('total stake of 0mint: {}, ration: {}'.format(sum_zero_stake, sum_zero_stake/ERC20DRK))
    dt.write()
