class Strategy(object):
    def __init__(self, epoch_len=0):
        self.epoch_len = epoch_len
        self.staked_tokens_ratio = 1

    def set_ratio(self, slot=0, apy=0):
        pass

    def staked_value(self, stake):
        return self.staked_tokens_ratio*stake

class RandomStrategy(Strategy):
    def __init__(self, epoch_len):
        Strategy.__init__(self, epoch_len)


    def set_ratio(self, slot, apy=0):
        if slot%self.epoch_len==0:
            self.staked_tokens_ratio = random.random()

class LinearStrategy(Strategy):
    '''
    linear staking strategy wrt apy.
    assume optimal is 20% APY!
    '''
    def __init__(self, epoch_len=0):
        Strategy.__init__(self, epoch_len)
        self.TARGET_APY = 20.0

    def set_ratio(self, slot, apy):
        if slot%self.epoch_len==0:
            self.staked_tokens_ratio = apy/self.TARGET_APY
