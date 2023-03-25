from lottery import *
import os
import numpy
os.system("rm f.hist; rm leads.hist")

RUNNING_TIME = int(input("running time:"))
NODES=1000
ERC20DRK = 2.1*10**9

if __name__ == "__main__":
    darkies = [ Darkie(random.gauss(ERC20DRK/NODES, ERC20DRK/NODES*0.1)) for id in range(NODES) ]
    airdrop = ERC20DRK
    print("network airdrop: {} on {} nodes".format(airdrop, len(darkies)))
    dt = DarkfiTable(airdrop, RUNNING_TIME, CONTROLLER_TYPE_DISCRETE, kp=-0.010399999999938556, ki=-0.0365999996461878, kd=0.03840000000000491)
    for darkie in darkies:
        dt.add_darkie(darkie)
    acc = dt.background(rand_running_time=False)
    print('acc: {}'.format(acc))
    dt.write()
