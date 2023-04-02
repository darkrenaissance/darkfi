from utils import *
import math

class Strategy(object):
    def __init__(self, epoch_len=0):
        self.epoch_len = epoch_len
        self.staked_tokens_ratio = Num(1)

    def set_ratio(self, slot=0, apy=0):
        pass

    def staked_value(self, stake):
        return self.staked_tokens_ratio*Num(stake)

class RandomStrategy(Strategy):
    def __init__(self, epoch_len):
        Strategy.__init__(self, epoch_len)


    def set_ratio(self, slot, apy=0):
        if slot%self.epoch_len==0 and slot>EPOCH_LENGTH:
            self.staked_tokens_ratio = random.random()
            #print('staked ratio: {}'.format(self.staked_tokens_ratio))

class LinearStrategy(Strategy):
    '''
    linear staking strategy wrt apy.
    assume optimal is 20% APY!
    '''
    def __init__(self, epoch_len=0):
        Strategy.__init__(self, epoch_len)
        self.TARGET_APY = 15

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0 and slot>EPOCH_LENGTH:
            self.staked_tokens_ratio = apy/Num(self.TARGET_APY)
            #print('staked ratio: {}'.format(self.staked_tokens_ratio))

class LogarithmicStrategy(Strategy):
    '''
    logarithmic staking strategy wrt apy.
    assume optimal is 20% APY!
    '''
    def __init__(self, epoch_len=0):
        Strategy.__init__(self, epoch_len)
        self.TARGET_APY = 15

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0 and slot>EPOCH_LENGTH:
            apy_ratio = math.fabs(apy/self.TARGET_APY)
            self.staked_tokens_ratio = Num((math.log(apy_ratio, 10)+1)/2 if apy_ratio != 0 else 0)
            #print('staked ratio: {}'.format(self.staked_tokens_ratio))

class SigmoidStrategy(Strategy):
    '''
    logarithmic staking strategy wrt apy.
    assume optimal is 20% APY!
    '''
    def __init__(self, epoch_len=0):
        Strategy.__init__(self, epoch_len)
        self.TARGET_APY = 10

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0 and slot>self.epoch_len:
            apy_ratio = math.fabs(apy/self.TARGET_APY)
            #print("apy ratio: {}".format(apy_ratio))
            self.staked_tokens_ratio = Num(2/(1+math.pow(math.e, -4*apy_ratio))-1)
            #print('ratio: {}, staked: {}'.format(apy_ratio, self.staked_tokens_ratio))
            assert(self.staked_tokens_ratio>=0 and self.staked_tokens_ratio<=1)
            #print('staked ratio: {}'.format(self.staked_tokens_ratio))
