from core.lottery import *
from core.strategy import random_strategy
from core.constants import *
from pid.pid_base import *
from draw import draw
import logging
import os

logging.basicConfig(filename='log/vesting.log', encoding='utf-8', level=logging.DEBUG)

def vesting_instance(vesting, exchange_rate, running_time):
    os.system("rm log/*_feedback.hist; rm log/*_output.hist log/darkie* log/rewards.log")
    native_drk = ERC20DRK * exchange_rate
    total_vesting = 0
    if __name__ == "__main__":
        darkies = []
        id = 0
        for name, distrib in vesting.items():
            darkies += [Darkie(distrib[0] , vesting=distrib, strategy=random_strategy(EPOCH_LENGTH))]
            id+=1
            total_vesting+=distrib[-1]
        airdrop = 0
        for darkie in darkies:
            airdrop+=darkie.stake
        print("initial airdrop: {}/{}% on {} nodes".format(airdrop, airdrop/native_drk*100, len(darkies)))
        print('total predistribution: {}/{}%'.format(total_vesting, total_vesting/native_drk*100))
        dt = DarkfiTable(airdrop, running_time, kp=-0.010399999999938556, ki=-0.0365999996461878, kd=0.03840000000000491,  r_kp=-2.53, r_ki=29.5, r_kd=53.77)
        for darkie in darkies:
            dt.add_darkie(darkie)
        dt.background(rand_running_time=False)
        dt.write()
    return dt.avg_apr()

if not os.path.exists(VESTING_FILE):
    print('add vested distribution csv at path {} with vesting period {} (slots) aparts.'.format(VESTING_FILE, ONE_MONTH))
    exit()

vesting = {}
with open(VESTING_FILE) as f:
    for node  in f.readlines():
        keyval = node.split(',')
        key = keyval[0]
        val = ','.join(keyval[1:])
        vesting[keyval[0]] = eval(eval(val))

nodes = len(vesting)
exchange_rate = float(input("enter exchange rate:"))
running_time = input("running time (leave it empty to run the whole vesting running time):")
if running_time=='':
    running_time = len(next(iter(vesting.values())))*VESTING_PERIOD
else:
    running_time = int(running_time)

apr = vesting_instance(vesting, exchange_rate, running_time)
print('avg apr: {}%'.format(apr*100))
draw()
