import math
import numpy as np
import matplotlib.pyplot as plt
import random

L = 28948022309329048855892746252171976963363056481941560715954676764349967630337

def target(f, rel_stake):
    T  = L * (1 - (1-f)**rel_stake)
    return T

def approx_target_in_zk(sigma_1, sigma_2, stake):
    # both sigma_1, sigma_2 are constants, if f is a constant.
    # if f is constant then sigma_12, sigma_2
    # this dictates that tuning need to be hardcoded,
    # secondly the reward, or at least the total stake in the network,
    # can't be anonymous, should be public.
    T = sigma_1 * stake + sigma_2*stake**2
    return T

def approx_target(f, stake, Sigma):
    x = (1-f)
    c = math.log(x)
    k = L*c
    kp = k*c
    kpp = kp/2
    # approx sigma
    sigma_1 = -1 * k/Sigma
    sigma_2 = -1 * kpp/(Sigma**2)
    # sigma is in Z
    sigma_2 = int(sigma_2)
    sigma_1 = int(sigma_1)
    stake = int(stake)
    return approx_target_in_zk(sigma_1, sigma_2, stake)


f = 0.5
# let's assume stakeholde having 1% of the stake, 1/100.
# each iteration increases stake by value 1.
TOTAL = 10000
S = []
stake = 0
T = []
T_approx = []
for i in range(TOTAL):
    if random.random()>=0.9:
        stake+=1
    S+=[(stake, i+1.0)]
    t = target(f, stake/(i+1.0))
    T+=[t]
    t_approx = approx_target(f, stake, (i+1.0))
    T_approx+=[t_approx]

plt.plot(T)
plt.plot(T_approx)
plt.legend(["target", "approximation"])
plt.savefig('plot.png')
