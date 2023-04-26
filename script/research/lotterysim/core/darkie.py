from core.utils import *
from core.strategy import *

class Darkie():
    def __init__(self, airdrop, initial_stake=None, vesting=[], hp=False, commit=True, epoch_len=EPOCH_LENGTH, strategy=random_strategy(EPOCH_LENGTH)):
        self.vesting = [0] + vesting
        self.stake = (Num(airdrop) if hp else airdrop)
        self.initial_stake = [self.stake] # for debugging purpose
        self.Sigma = None
        self.feedback = None
        self.f = None
        self.epoch_len=epoch_len # epoch length during which the stake is static
        self.strategy = strategy
        self.slot = 0
        self.won_hist = [] # winning history boolean

    def clone(self):
        return Darkie(self.stake)

    def apy_scaled_to_runningtime(self, rewards):
        avg_apy = 0
        for idx, reward in enumerate(rewards):
            current_epoch_staked_tokens = Num(self.strategy.staked_tokens_ratio[idx-1]) * Num(self.initial_stake[idx-1])
            avg_apy += (Num(reward) / current_epoch_staked_tokens) if current_epoch_staked_tokens!=0 else 0
        return avg_apy * Num(ONE_YEAR/(self.slot/EPOCH_LENGTH)) if self.slot  and self.initial_stake[0]>0 >0 else 0


    def apr_scaled_to_runningtime(self):
        return Num(self.stake - self.initial_stake[0]) / Num(self.initial_stake[0]) *  Num(ONE_YEAR/(self.slot/EPOCH_LENGTH)) if self.slot> 0 and self.initial_stake[0]>0 else 0

    def staked_tokens(self):
        '''
        the ratio of the staked tokens during the epochs
        of the total running time
        '''
        return Num(self.initial_stake[0])*self.staked_tokens_ratio()


    def staked_tokens_ratio(self):
        staked_ratio = Num(sum(self.strategy.staked_tokens_ratio)/len(self.strategy.staked_tokens_ratio))
        #print('type: {}, ratio: {}'.format(self.strategy.type, staked_ratio))
        assert staked_ratio <= 1 and staked_ratio >=0, 'staked_ratio: {}'.format(staked_ratio)
        return staked_ratio


    def set_sigma_feedback(self, sigma, feedback, f, count, hp=True):
        self.Sigma = (Num(sigma) if hp else sigma)
        self.feedback = (Num(feedback) if hp else feedback)
        self.f = (Num(f) if hp else f)
        self.slot = count


    def run(self, hp=True):
        k=N_TERM
        def target(tune_parameter, stake):
            x = (Num(1) if hp else 1)  - (Num(tune_parameter) if hp else tune_parameter)
            c = (x.ln() if type(x)==Num else math.log(x))
            sigmas = [   c/((self.Sigma+EPSILON)**i) * ( ((L_HP if hp else L)/fact(i)) ) for i in range(1, k+1) ]
            scaled_target = approx_target_in_zk(sigmas, Num(stake)) #+ (BASE_L_HP if hp else BASE_L)
            return scaled_target

        if self.slot % EPOCH_LENGTH ==0 and self.slot > 0:
            apr = self.apr_scaled_to_runningtime()
            # staked ratio is added in strategy
            self.strategy.set_ratio(self.slot, apr)
            # epoch stake is added
            self.initial_stake +=[self.stake]
        T = target(self.f, self.strategy.staked_value(self.stake))
        won = lottery(T, hp)
        self.won_hist += [won]

    def update_vesting(self):
        if self.slot >= len(self.vesting):
            return 0
        slot2vest_index = int(self.slot/28800.0) #slot to month conversion
        slot2vest_prev_index = int((self.slot-1)/28800.0)
        slot2vest_index_shifted = slot2vest_index - 1 # by end of month
        slot2vest_prev_index_shifted = slot2vest_prev_index - 1 # by end of month
        vesting_value = float(self.vesting[slot2vest_index_shifted]) - self.vesting[slot2vest_prev_index_shifted]
        self.stake+= vesting_value
        return vesting_value

    def update_stake(self, reward):
        if self.won_hist[-1]:
            self.stake+=reward
            #print('updating stake, stake: {}, last: {}'.format(self.stake, self.initial_stake[-1]))

    def resync_stake(self, reward):
        '''
        add resync stake
        '''
        self.stake += reward


    def write(self, idx):
        with open('log/darkie'+str(idx)+'.log', 'w+') as f:
            buf = 'initial stake:'+','.join([str(i) for i in self.initial_stake])
            buf += '\r\n'
            buf += '(apr,staked ratio,{}):'.format(self.strategy.type)+','.join(['('+str(apr)+','+str(sr)+')' for sr, apr in zip(self.strategy.staked_tokens_ratio, self.strategy.annual_return)])
            buf+='\r\n'
            buf += 'apr: {}'.format(self.apr_scaled_to_runningtime())
            f.write(buf)
