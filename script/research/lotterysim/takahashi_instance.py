from core.lottery import *
from core.utils import *
from core.strategy import *

NODES=1000

RUNNING_TIME = int(input("running time:"))
if __name__ == "__main__":
    darkies = [Darkie(i, strategy=random_strategy(EPOCH_LENGTH), apy_window=EPOCH_LENGTH) for i in range(NODES)]
    airdrop = 0
    for darkie in darkies:
        airdrop+=darkie.stake
    dt  = DarkfiTable(airdrop, RUNNING_TIME, controller_type=CONTROLLER_TYPE_TAKAHASHI, kc=-2.19, ti=-0.5, td=0.25, ts=-0.35,  r_kp=-0.42, r_ki=2.71, r_kd=-0.239)
    for darkie in darkies:
        dt.add_darkie(darkie)
    acc, apy, reward, stake_ratio, apr = dt.background()
    print('acc: {}'.format(acc))
    dt.write()
