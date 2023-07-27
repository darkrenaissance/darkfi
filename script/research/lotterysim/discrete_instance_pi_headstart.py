import os
import numpy
from core.strategy import *
from core.lottery import *
import matplotlib.pyplot as plt
import scipy.stats as stats
import math
from draw import draw

os.system("rm log/*_feedback.hist; rm log/*_output.hist; rm log/darkie*.log")

RUNNING_TIME = int(input("running time:"))


if __name__ == "__main__":
    mu = PREMINT/NODES
    darkies = [Darkie(random.gauss(mu, mu/10), strategy=random_strategy(EPOCH_LENGTH)) for _ in range(NODES)]
    #dt = DarkfiTable(0, RUNNING_TIME, CONTROLLER_TYPE_DISCRETE, kp=-0.010399999999938556, ki=-0.0365999996461878, kd=0.03840000000000491,  r_kp=-2.53, r_ki=29.5, r_kd=53.77)
    dt = DarkfiTable(PREMINT, RUNNING_TIME, CONTROLLER_TYPE_DISCRETE, kp=-0.010399999999938556, ki=-0.0365999996461878, kd=0.03840000000000491,  r_kp=-0.719, r_ki=1.6, r_kd=0.1)
    for darkie in darkies:
        dt.add_darkie(darkie)
    acc, avg_apy, avg_reward, stake_ratio, avg_apr = dt.background(rand_running_time=False)
    #sum_zero_stake = sum([darkie.stake for darkie in darkies[NODES:]])
    print('acc: {}, avg(apr): {}%, avg(reward): {}, stake_ratio: {}'.format(acc, round(avg_apr*100,2), avg_reward, stake_ratio))
    #print('total stake of 0mint: {}, ratio: {}'.format(sum_zero_stake, sum_zero_stake/ERC20DRK))
    dt.write()
    aprs = []
    fortuners = 0.0
    for darkie in darkies:
        aprs += [float(darkie.apr_scaled_to_runningtime())]
        if darkie.initial_stake[-1] - darkie.initial_stake[0] > 0:
            fortuners+=1

    print('fortuners: {}'.format(str(fortuners/len(darkies))))
    # distribution of aprs
    aprs = sorted(aprs)
    mu = float(sum(aprs)/len(aprs))
    shifted_aprs = [apr - mu for apr in aprs]
    plt.plot([round(apr*100,2) for apr in aprs])
    plt.title('annual percentage return, avg: {:}'.format(mu*100))
    plt.savefig('img/apr_distribution.png')
    plt.show()

    variance = sum(shifted_aprs)/(len(aprs)-1)
    print('mu: {}, variance: {}'.format(str(mu), str(variance)))
    draw()
