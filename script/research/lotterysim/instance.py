from lottery import *
import os
import numpy
os.system("rm f.hist; rm leads.hist")

RUNNING_TIME = int(input("running time:"))


if __name__ == "__main__":
    darkies = []
    darkies += [ Darkie(random.gauss(20,20)*50) for id in range(1000000) ]
    airdrop = 0
    for darkie in darkies:
        airdrop+=darkie.stake
    print("network airdrop: {} on {} nodes".format(airdrop, len(darkies)))
    dt = DarkfiTable(airdrop, RUNNING_TIME, CONTROLLER_TYPE_DISCRETE, kp=0.005999999999989028, ki=-0.005999999985257798, kd=0.01299999999999478)
    for darkie in darkies:
        dt.add_darkie(darkie)
    acc = dt.background(rand_running_time=False)
    print('acc: {}'.format(acc))
    dt.write()
