from utils import *
import math

class Strategy(object):
    def __init__(self, epoch_len=0):
        self.epoch_len = epoch_len
        self.staked_tokens_ratio = [1]
        self.target_apy = TARGET_APY
        self.type = 'base'

    def set_ratio(self, slot=0, apy=0):
        pass

    def staked_value(self, stake):
        #assert(self.staked_tokens_ratio[-1]>=0 and self.staked_tokens_ratio[-1]<=1)
        return Num(self.staked_tokens_ratio[-1])*Num(stake)

class RandomStrategy(Strategy):
    def __init__(self, epoch_len):
        Strategy.__init__(self, epoch_len)
        self.type = 'random'


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
        self.type = 'linear'

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0 and slot>EPOCH_LENGTH:
            self.staked_tokens_ratio += [Num(apy)/Num(self.target_apy)]

class LogarithmicStrategy(Strategy):
    '''
    logarithmic staking strategy wrt apy.
    assume optimal is 20% APY!
    '''
    def __init__(self, epoch_len=0):
        Strategy.__init__(self, epoch_len)
        self.type = 'logarithmic'

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0 and slot>EPOCH_LENGTH:
            apy_ratio = math.fabs(apy/self.target_apy)
            fn = lambda x: (math.log(x, 10)+1)/2 * 0.95 + 0.05
            print('apy_ratio: {}, output: {}'.format(apy_ratio, fn(apy_ratio)))
            self.staked_tokens_ratio += [Num(fn(apy_ratio) if apy_ratio != 0 else 0)]


class SigmoidStrategy(Strategy):
    '''
    logarithmic staking strategy wrt apy.
    assume optimal is 20% APY!
    '''
    def __init__(self, epoch_len=0):
        Strategy.__init__(self, epoch_len)
        self.type = 'sigmoid'

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0 and slot>self.epoch_len:
            apy_ratio = math.fabs(apy/self.target_apy)
            self.staked_tokens_ratio += [Num(2/(1+math.pow(math.e, -4*apy_ratio))-1)]


def random_strategy(epoch_length):
    rnd = random.random()
    if rnd < 0.25:
        return RandomStrategy(epoch_length)
    elif rnd < 0.5 and rnd >=0.25:
        return LinearStrategy(epoch_length)
    else:
        return SigmoidStrategy(epoch_length)
