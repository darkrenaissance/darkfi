from utils import *
import math

class Strategy(object):
    def __init__(self, epoch_len=0):
        self.epoch_len = epoch_len
        self.staked_tokens_ratio = [Num(1)]
        self.target_apy = TARGET_APY

    def set_ratio(self, slot=0, apy=0):
        pass

    def staked_value(self, stake):
        assert(self.staked_tokens_ratio[-1]>=0 and self.staked_tokens_ratio[-1]<=1)
        return self.staked_tokens_ratio[-1]*Num(stake)

class RandomStrategy(Strategy):
    def __init__(self, epoch_len):
        Strategy.__init__(self, epoch_len)


    def set_ratio(self, slot, apy=0):
        if slot%self.epoch_len==0 and slot>EPOCH_LENGTH:
            self.staked_tokens_ratio += [random.random()]

class LinearStrategy(Strategy):
    '''
    linear staking strategy wrt apy.
    assume optimal is 20% APY!
    '''
    def __init__(self, epoch_len=0):
        Strategy.__init__(self, epoch_len)

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0 and slot>EPOCH_LENGTH:
            self.staked_tokens_ratio += [apy/Num(self.target_apy) * Num(0.9) + Num(0.1)]

class LogarithmicStrategy(Strategy):
    '''
    logarithmic staking strategy wrt apy.
    assume optimal is 20% APY!
    '''
    def __init__(self, epoch_len=0):
        Strategy.__init__(self, epoch_len)

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0 and slot>EPOCH_LENGTH:
            apy_ratio = math.fabs(apy/self.target_apy)
            self.staked_tokens_ratio += [Num((math.log(apy_ratio, 10)+1)/2 if apy_ratio != 0 else 0)]


class SigmoidStrategy(Strategy):
    '''
    logarithmic staking strategy wrt apy.
    assume optimal is 20% APY!
    '''
    def __init__(self, epoch_len=0):
        Strategy.__init__(self, epoch_len)

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0 and slot>self.epoch_len:
            apy_ratio = math.fabs(apy/self.target_apy)
            self.staked_tokens_ratio += [Num(2/(1+math.pow(math.e, -4*apy_ratio))-1)]
