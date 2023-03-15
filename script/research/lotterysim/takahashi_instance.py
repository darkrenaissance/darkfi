from lottery import *

NODES=1000

RUNNING_TIME = int(input("running time:"))
if __name__ == "__main__":
    darkies = [Darkie(i) for i in range(NODES)]
    airdrop = 0
    for darkie in darkies:
        airdrop+=darkie.stake
    dt  = DarkfiTable(airdrop, RUNNING_TIME, controller_type=CONTROLLER_TYPE_TAKAHASHI, kc=-2.19, ti=-0.5, td=0.25, ts=-0.35)
    for darkie in darkies:
        dt.add_darkie(darkie)
    acc = dt.background(True, False)
    print('acc: {}'.format(acc))
    dt.write()
