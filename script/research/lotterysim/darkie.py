from utils import *
from threading import Thread

class Darkie(Thread):
    def __init__(self, airdrop, vesting=[], hp=False):
        Thread.__init__(self)
        self.vesting = [0] + vesting
        self.stake = (Num(airdrop) if hp else airdrop)
        self.finalized_stake = (Num(airdrop) if hp else airdrop) # after fork finalization
        self.Sigma = None
        self.feedback = None
        self.f = None
        self.won=False

    def clone(self):
        return Darkie(self.finalized_stake)

    def set_sigma_feedback(self, sigma, feedback, f, hp=False):
        self.Sigma = (Num(sigma) if hp else sigma)
        self.feedback = (Num(feedback) if hp else feedback)
        self.f = (Num(f) if hp else f)

    def run(self, hp=False):
        k=N_TERM
        def target(tune_parameter, stake):
            x = (Num(1) if hp else 1)  - (Num(tune_parameter) if hp else tune_parameter)
            c = (x.ln() if type(x)==Num else math.log(x))
            sigmas = [   c/((self.Sigma+EPSILON)**i) * ( ((L_HP if hp else L)/fact(i)) ) for i in range(1, k+1) ]
            scaled_target = approx_target_in_zk(sigmas, stake) #+ (BASE_L_HP if hp else BASE_L)
            return scaled_target
        T = target(self.f, self.finalized_stake)
        self.won = lottery(T, hp)

    def update_vesting(self, slot):
        if slot >= len(self.vesting):
            return 0
        slot2vest_index = int(slot/28800.0)
        slot2vest_prev_index = int((slot-1)/28800.0)
        slot2vest_index_shifted = slot2vest_index - 1 # by end of month
        slot2vest_prev_index_shifted = slot2vest_prev_index - 1 # by end of month
        vesting_value = float(self.vesting[slot2vest_index_shifted]) - self.vesting[slot2vest_prev_index_shifted]
        self.stake+= vesting_value
        return vesting_value

    def update_stake(self):
        self.stake+=REWARD

    def finalize_stake(self):
        if self.won:
            self.finalized_stake = self.stake
        else:
            self.stake = self.finalized_stake
