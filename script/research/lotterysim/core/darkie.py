from core.utils import *
from core.strategy import *

class Darkie():
    def __init__(self, airdrop, initial_stake=None, vesting=[], hp=False, commit=True, epoch_len=EPOCH_LENGTH, strategy=random_strategy(EPOCH_LENGTH)):
        self.vesting = vesting
        self.stake = (Num(airdrop) if hp else airdrop)
        self.initial_stake = [self.stake]
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
            init_stake = Num(self.initial_stake[idx-1]) if len(self.initial_stake)>=idx else Num(self.initial_stake[-1])
            current_epoch_staked_tokens = Num(self.strategy.staked_tokens_ratio[idx-1]) * init_stake
            avg_apy += (Num(reward) / current_epoch_staked_tokens) if current_epoch_staked_tokens!=0 else 0
        return avg_apy * Num(ONE_YEAR/(self.slot/EPOCH_LENGTH)) if self.slot  and self.initial_stake[0]>0 >0 else 0

    def vesting_wrapped_initial_stake(self):
        #print('initial stake: {}, corresponding vesting: {}'.format(self.initial_stake[0], self.vesting[int((self.slot)/VESTING_PERIOD)]))
        # note index is previous slot since update_vesting is called after background execution.
        #return self.current_vesting() if self.slot>0 else self.initial_stake[-1]
        return (self.current_vesting() if self.slot>0 else self.initial_stake[-1]) + self.initial_stake[-1]

    def apr_scaled_to_runningtime(self):
        initial_stake = self.vesting_wrapped_initial_stake()
        #print('stake: {}, initial_stake: {}'.format(self.stake, initial_stake))
        assert self.stake >= initial_stake, 'stake: {}, initial_stake: {}, slot: {}, current: {}, previous: {} vesting'.format(self.stake, initial_stake, self.slot, self.current_vesting(), self.prev_vesting())
        #if self.slot%100==0:
            #print('stake: {}, initial stake: {}'.format(self.stake, initial_stake))
            #print(self.initial_stake)
        apr = Num(self.stake - initial_stake) / Num(initial_stake) *  Num(ONE_YEAR/(self.slot)) if self.slot> 0 and initial_stake>0 else 0
        return apr

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
            scaled_target = approx_target_in_zk(sigmas, Num(stake)) + ((BASE_L_HP if hp else BASE_L) if self.slot < HEADSTART_AIRDROP else 0)
            return scaled_target

        if self.slot % EPOCH_LENGTH ==0 and self.slot > 0:
            apr = self.apr_scaled_to_runningtime()
            # staked ratio is added in strategy
            self.strategy.set_ratio(self.slot, apr)
            # epoch stake is added
            if self.slot < HEADSTART_AIRDROP:
                self.initial_stake +=[self.stake]
        #if self.slot == HEADSTART_AIRDROP:
        #    self.initial_stake += [self.stake]
        T = target(self.f, self.strategy.staked_value(self.stake))
        won = lottery(T, hp)
        self.won_hist += [won]

    def update_vesting(self):
        self.stake += self.vesting_differential()

    def current_vesting(self):
        '''
        current corresponding slot vesting
        '''
        vesting_idx = int(self.slot/VESTING_PERIOD)
        return self.vesting[vesting_idx] if vesting_idx < len(self.vesting) else 0

    def prev_vesting(self):
        '''
        previous corresponding slot vesting
        '''
        prev_vesting_idx = int((self.slot-1)/VESTING_PERIOD)
        return (self.vesting[prev_vesting_idx] if self.slot>0 else self.current_vesting()) if prev_vesting_idx < len(self.vesting) else  0

    def vesting_differential(self):
        vesting_value =  self.current_vesting() - self.prev_vesting()
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
