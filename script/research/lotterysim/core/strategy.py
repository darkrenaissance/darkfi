import math
from core.utils import *

class Strategy(object):
    '''
    @type epoch_len: int
    @epoch_len: epoch length
    @type airdrop_period: int
    @param airdrop_period: strategy grace period, during which strategy is HODL only
    '''
    def __init__(self, epoch_len=0, airdrop_period=HEADSTART_AIRDROP):
        self.epoch_len = epoch_len
        self.airdrop_period=HEADSTART_AIRDROP
        self.staked_tokens_ratio = [1]
        self.target = TARGET_APR
        self.annual_return = [0]
        self.type = 'base'

    def set_ratio(self, slot, apr):
        return

    def staked_value(self, stake):
        return (self.staked_tokens_ratio[-1])*(stake)

class Hodler(Strategy):
    def __init__(self, epoch_len):
        super().__init__(epoch_len)
        self.type = 'hodler'

    def set_ratio(self, slot, apr):
        if slot%self.epoch_len==0:
            self.staked_tokens_ratio += [1]
            self.annual_return +=[apr]

class LinearStrategy(Strategy):
    def __init__(self, epoch_len=0):
        super().__init__(epoch_len)
        self.type = 'linear'

    def set_ratio(self, slot, apr):
        if slot%self.epoch_len==0:
                sr = (apr)/(self.target)
                if sr>1:
                    sr = 1
                elif sr<0:
                    sr = 0
                self.staked_tokens_ratio += [sr]
                self.annual_return += [apr]

class LogarithmicStrategy(Strategy):
    def __init__(self, epoch_len=0):
        super().__init__(epoch_len)
        self.type = 'logarithmic'

    def set_ratio(self, slot, apr):
        if slot%self.epoch_len==0:
                apr_ratio = math.fabs(apr/self.target)
                fn = lambda x: (math.log(x, 10)+1)/2 * 0.95 + 0.05
                sr = (fn(apr_ratio) if apr_ratio != 0 else 0)
                if sr>1:
                    sr = 1
                elif sr<0:
                    sr = 0
                self.staked_tokens_ratio += [sr]
                self.annual_return += [apr]

class SigmoidStrategy(Strategy):
    def __init__(self, epoch_len=0):
        super().__init__(epoch_len)
        self.type = 'sigmoid'

    def set_ratio(self, slot, apr):
        if slot%self.epoch_len==0:
                apr_ratio = apr/self.target
                apr_ratio = max(apr_ratio, 0)
                sr = (2/(1+math.pow(math.e, -4*apr_ratio))-1)
                if sr>1:
                    sr = 1
                elif sr<0:
                    sr = 0
                self.staked_tokens_ratio += [sr]
                self.annual_return += [apr]

def random_strategy(epoch_length=EPOCH_LENGTH):
    rnd = random.random()
    if rnd < 0.25:
        return Hodler(epoch_length)
    elif rnd < 0.5 and rnd >= 0.25:
        return LinearStrategy(epoch_length)
    elif rnd < 0.75 and rnd >= 0.5:
        return LogarithmicStrategy(epoch_length)
    else:
        return SigmoidStrategy(epoch_length)


class Tip(object):
    def __init__(self):
        self.type = 'tip'

    def get_tip(self, last_reward, apr, size, last_tip):
        return 0

class ZeroTip(Tip):

    def __init__(self):
        super().__init__()
        self.type = 'zero'

    def get_tip(self, last_reward, apr, size, last_tip):
        return 0

class TenthOfReward(Tip):
    def __init__(self):
        super().__init__()
        self.type = '10th'

    def get_tip(self, last_reward, apr, size, last_tip):
        return last_reward/10

class HundredthOfReward(Tip):
    def __init__(self):
        super().__init__()
        self.type = '100th'

    def get_tip(self, last_reward, apr, size, last_tip):
        return last_reward/100

class MilthOfReward(Tip):
    def __init__(self):
        super().__init__()
        self.type = '1000th'

    def get_tip(self, last_reward, apr, size, last_tip):
        return last_reward/1000

class RewardApr(Tip):
    def __init__(self):
        super().__init__()
        self.type = 'reward_apr'

    def get_tip(self, last_reward, apr, size, last_tip):
        apr_relu = max(apr, 0)
        apr_relu = min(apr_relu, 1)
        return last_reward*apr_relu

class TenthRewardApr(Tip):
    def __init__(self):
        super().__init__()
        self.type = 'reward_apr'

    def get_tip(self, last_reward, apr, size, last_tip):
        apr_relu = max(apr, 0)
        apr_relu = min(apr_relu, 1)
        return last_reward*apr_relu/10


class TenthCCApr(Tip):
    def __init__(self):
        super().__init__()
        self.type = "cc_apr_10"

    def get_tip(self, last_reward, apr, size, last_tip):
        return size/MAX_BLOCK_SIZE/10

class HundredthCCApr(Tip):
    def __init__(self):
        super().__init__()
        self.type = "cc_apr_100"

    def get_tip(self, last_reward, apr, size, last_tip):
        return size/MAX_BLOCK_SIZE/100

class MilthCCApr(Tip):
    def __init__(self):
        super().__init__()
        self.type = "cc_apr_1000"

    def get_tip(self, last_reward, apr, size, last_tip):
        return size/MAX_BLOCK_SIZE/1000

class Conservative(Tip):
    def __init__(self):
        super().__init__()
        self.type = "cc_apr_1000"

    def get_tip(self, last_reward, apr, size, last_tip):
        return last_tip

class Generous(Tip):
    def __init__(self):
        super().__init__()
        self.type = "cc_apr_1000"

    def get_tip(self, last_reward, apr, size, last_tip):
        return last_tip*2


def random_tip_strategy():
    return random.choice([ZeroTip(),  MilthOfReward(), MilthCCApr(), Conservative(), Generous()])
