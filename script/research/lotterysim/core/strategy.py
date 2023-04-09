import math
from core.utils import *

class Strategy(object):
    def __init__(self, epoch_len=0):
        self.epoch_len = epoch_len
        self.staked_tokens_ratio = [1]
        self.target_apy = TARGET_APR
        self.type = 'base'

    def set_ratio(self, slot, apy):
        return

    def staked_value(self, stake):
        return Num(self.staked_tokens_ratio[-1])*Num(stake)

class RandomStrategy(Strategy):
    def __init__(self, epoch_len):
        super().__init__(epoch_len)
        self.type = 'random'

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0:
            self.staked_tokens_ratio += [random.random()]

class LinearStrategy(Strategy):
    def __init__(self, epoch_len=0):
        super().__init__(epoch_len)
        self.type = 'linear'

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0:
            sr = Num(apy)/Num(self.target_apy)
            if sr>1:
                sr = 0
            elif sr<0:
                sr = 0
            self.staked_tokens_ratio += [sr]


class LogarithmicStrategy(Strategy):
    def __init__(self, epoch_len=0):
        super().__init__(epoch_len)
        self.type = 'logarithmic'

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0:
            apy_ratio = math.fabs(apy/self.target_apy)
            fn = lambda x: (math.log(x, 10)+1)/2 * 0.95 + 0.05
            sr = Num(fn(apy_ratio) if apy_ratio != 0 else 0)
            if sr>1:
                sr = 0
            elif sr<0:
                sr = 0
            self.staked_tokens_ratio += [sr]

class SigmoidStrategy(Strategy):
    def __init__(self, epoch_len=0):
        super().__init__(epoch_len)
        self.type = 'sigmoid'

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0:
            #TODO should be abs?
            apy_ratio = apy/self.target_apy
            sr = Num(2/(1+math.pow(math.e, -4*apy_ratio))-1)
            if sr>1:
                sr = 0
            elif sr<0:
                sr = 0
            self.staked_tokens_ratio += [sr]

def random_strategy(epoch_length):
    rnd = random.random()
    if rnd < 0.25:
        return RandomStrategy(epoch_length)
    elif rnd < 0.5 and rnd >=0.25:
        return LinearStrategy(epoch_length)
    else:
        return SigmoidStrategy(epoch_length)
