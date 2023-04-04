import random
import math
import numpy as np
from constants import *

# naive factorial
def fact(n, hp=False):
    assert (n>0)
    n = int(n)
    if n==1:
        return Num(1) if hp else 1
    elif n==2:
        return Num(2) if hp else 2
    else:
        return (Num(n) if hp else n)* fact(n-1, hp)

# all inputs to this function are integers
# sigmas are public
# stake is private
def approx_target_in_zk(sigmas, stake):
    # both sigma_1, sigma_2 are constants, if f is a constant.
    # if f is constant then sigma_12, sigma_2
    # this dictates that tuning need to be hardcoded,
    # secondly the reward, or at least the total stake in the network,
    # can't be anonymous, should be public.
    T = 0
    for i, sigma in  enumerate(sigmas):
        try:
            T += Num(sigma)*Num(stake)**(i+1)
        except Exception as e:
            T +=0

    return-1*T

def rnd(hp=False):
    return Num(random.random()) if hp else random.random()

def lottery(T, hp=False, log=False):
    y =  rnd(hp) * (L_HP if hp else L)
    if log:
        lottery_line = str(y)+","+str(T)+"\n"
        with open("/tmp/sim_lottery_history.log", "a+") as f:
            f.write(lottery_line)
    return y < T
