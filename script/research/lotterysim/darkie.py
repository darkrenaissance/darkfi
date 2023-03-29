from utils import *
from threading import Thread
from strategy import *

class Darkie(Thread):
    def __init__(self, airdrop, initial_stake=None, vesting=[], hp=False, commit=True, epoch_len=100, strategy=None, apy_window=0):
        Thread.__init__(self)
        self.vesting = [0] + vesting
        self.stake = (Num(airdrop) if hp else airdrop)
        self.initial_stake = [self.stake] # for debugging purpose
        self.finalized_stake = (Num(airdrop) if hp else airdrop)
        self.Sigma = None
        self.feedback = None
        self.f = None
        self.won=False
        self.epoch_len=epoch_len # epoch length during which the stake is static
        self.strategy = strategy if strategy is not None else Strategy(self.epoch_len)
        self.apy_window = apy_window
        self.staked_tokens_ratio = 1 # ratio of staked tokens, if commit is true then it's 100%
        self.slot = 0

    def clone(self):
        return Darkie(self.finalized_stake)

    def apy(self):
        if self.apy_window == 0:
            window=len(self.initial_stake)
        # approximation to APY assuming linear relation
        # note! relation is logarithmic depending on PID output.
        if window<len(self.initial_stake):
            windowed_initial_stake = self.initial_stake[-window]
            return Num(self.stake - windowed_initial_stake) / Num(windowed_initial_stake) if self.stake>0 else Num(0)
        return Num(self.stake - self.initial_stake[0]) / Num(self.initial_stake[0]) if self.stake>0 else Num(0)

    def apy_percentage(self):
        return self.apy()*100

    def set_sigma_feedback(self, sigma, feedback, f, count, hp=True):
        self.Sigma = (Num(sigma) if hp else sigma)
        self.feedback = (Num(feedback) if hp else feedback)
        self.f = (Num(f) if hp else f)
        self.initial_stake += [self.finalized_stake]
        self.slot = count

    def run(self, hp=True):
        k=N_TERM
        def target(tune_parameter, stake):
            x = (Num(1) if hp else 1)  - (Num(tune_parameter) if hp else tune_parameter)
            c = (x.ln() if type(x)==Num else math.log(x))
            sigmas = [   c/((self.Sigma+EPSILON)**i) * ( ((L_HP if hp else L)/fact(i)) ) for i in range(1, k+1) ]
            scaled_target = approx_target_in_zk(sigmas, Num(stake)) #+ (BASE_L_HP if hp else BASE_L)
            return scaled_target

        self.strategy.set_ratio(self.slot, self.apy())
        T = target(self.f, self.strategy.staked_value(self.finalized_stake))
        self.won = lottery(T, hp)

    def update_vesting(self):
        if self.slot >= len(self.vesting):
            return 0
        slot2vest_index = int(self.slot/28800.0)
        slot2vest_prev_index = int((self.slot-1)/28800.0)
        slot2vest_index_shifted = slot2vest_index - 1 # by end of month
        slot2vest_prev_index_shifted = slot2vest_prev_index - 1 # by end of month
        vesting_value = float(self.vesting[slot2vest_index_shifted]) - self.vesting[slot2vest_prev_index_shifted]
        self.stake+= vesting_value
        return vesting_value

    def update_stake(self, reward):
        if self.won:
            self.stake+=reward

    def finalize_stake(self):
        if self.won:
            self.finalized_stake = self.stake
        else:
            self.stake = self.finalized_stake

    def log_state_gain(self):
        # darkie started with self.initial_stake, self.initial_stake/self.Sigma percent
        # over the course of self.slot
        # current stake is self.stake, self.stake/self.Sigma percent
        pass
