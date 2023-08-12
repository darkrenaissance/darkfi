from core.utils import *
from core.strategy import *

class Darkie():
    def __init__(self, airdrop, initial_stake=None, vesting=[], hp=False, commit=True, epoch_len=EPOCH_LENGTH, strategy=random_strategy(EPOCH_LENGTH), idx=0):
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
        self.fees = []
        self.tips = [0]
        self.idx=idx
        self.aprs = [] #milinial aprs. every 1k slots ~ 1day

    def clone(self):
        return Darkie(self.stake)

    """
    calculate APY (with compound interest every epoch) every epoch scaled to runningtime
    @param rewards: rewards at each epoch
    @returns: apy
    """
    def apy_scaled_to_runningtime(self, rewards):
        avg_apy = 0
        for idx, reward in enumerate(rewards):
            #init_stake = Num(self.initial_stake[idx-1]) if len(self.initial_stake)>=idx else Num(self.initial_stake[-1])
            current_epoch_staked_tokens = Num(self.strategy.staked_tokens_ratio[idx-1]) * Num(self.initial_stake[idx-1])
            avg_apy += (Num(reward) / current_epoch_staked_tokens) if current_epoch_staked_tokens!=0 else 0
        return avg_apy * Num(ONE_YEAR/(self.slot/EPOCH_LENGTH)) if self.slot  and self.initial_stake[0]>0 >0 else 0

    """
    calculate APR every epoch scaled to running time
    @returns: apr
    """
    def apr_scaled_to_runningtime(self):
        initial_stake = self.vesting_wrapped_initial_stake()
        #assert self.stake >= initial_stake, 'stake: {}, initial_stake: {}, slot: {}, current: {}, previous: {} vesting'.format(self.stake, initial_stake, self.slot, self.current_vesting(), self.prev_vesting())
        if self.slot < HEADSTART_AIRDROP:
            # during this phase, it's only called at end of epoch
            apr_period = self.slot%EPOCH_LENGTH
        elif self.slot > MIL_SLOT:
            apr_period = self.slot - int(self.slot/MIL_SLOT)*MIL_SLOT
        else:
            apr_period = self.slot-HEADSTART_AIRDROP
        apr_scaled = ((self.stake - initial_stake) / initial_stake) / apr_period if initial_stake>0  and apr_period>0 else 0
        apr = apr_scaled * ONE_YEAR if initial_stake > 0 and apr_period>0 and self.slot>0 else 0
        #if apr>0 and self.stake-initial_stake>0:
            #print("apr: {}, stake: {}, initial_stake: {}".format(apr, self.stake, initial_stake))
        return apr


    """
    add vesting to initial stake
    @returns: vesting plus initial stake
    """
    def vesting_wrapped_initial_stake(self):
        #returns  vesting stake plus initial stake gained from zero coin headstart during aridrop period
        vesting = self.current_vesting()
        if self.slot < HEADSTART_AIRDROP:
            initial_stake = self.initial_stake[-1]
        elif self.slot > MIL_SLOT:
            initial_stake = self.initial_stake[int(int(self.slot/MIL_SLOT) * MIL_SLOT/EPOCH_LENGTH)-1]
        else:
            initial_stake = self.initial_stake[int(HEADSTART_AIRDROP/EPOCH_LENGTH)-1]
        return vesting + initial_stake

    """
    update stake with vesting return every scheduled vesting period
    """
    def update_vesting(self):
        diff = self.vesting_differential()
        self.stake += diff
        return diff

    """
    @returns: current epoch vesting
    """
    def current_vesting(self):
        '''
        current corresponding slot vesting
        '''
        vesting_idx = int(self.slot/VESTING_PERIOD)
        return self.vesting[vesting_idx] if vesting_idx < len(self.vesting) else 0

    """
    @returns: previous epoch vesting
    """
    def prev_vesting(self):
        '''
        previous corresponding slot vesting
        '''
        prev_vesting_idx = int((self.slot-1)/VESTING_PERIOD)
        return (self.vesting[prev_vesting_idx] if self.slot>0 else self.current_vesting()) if prev_vesting_idx < len(self.vesting) else  0

    def vesting_differential(self):
        vesting_value =  self.current_vesting() - self.prev_vesting()
        return vesting_value

    def staked_tokens(self):
        '''
        the ratio of the staked tokens during the epochs
        of the total running time
        '''
        return Num(self.initial_stake[0])*self.staked_tokens_ratio()

    """
    @returns: average stakeholder's staked ratio from genesis until current slot
    """
    def staked_tokens_ratio(self):
        staked_ratio = Num(sum(self.strategy.staked_tokens_ratio)/len(self.strategy.staked_tokens_ratio))
        assert staked_ratio <= 1 and staked_ratio >=0, 'staked_ratio: {}'.format(staked_ratio)
        return staked_ratio


    def set_sigma_feedback(self, sigma, feedback, f, count, hp=True):
        self.Sigma = (Num(sigma) if hp else sigma)
        self.feedback = (Num(feedback) if hp else feedback)
        self.f = (Num(f) if hp else f)
        self.slot = count

    """
    @param hp: high precision decimal option
    play lottery if stakeholder won, update state
    """
    def run(self, hp=True):
        k=N_TERM
        def target(tune_parameter, stake):
            x = (Num(1) if hp else 1)  - (Num(tune_parameter) if hp else tune_parameter)
            c = (x.ln() if type(x)==Num else math.log(x))
            sigmas = [   c/((self.Sigma+EPSILON)**i) * ( ((L_HP if hp else L)/fact(i)) ) for i in range(1, k+1) ]
            headstart = (BASE_L_HP if hp else BASE_L) if self.slot < HEADSTART_AIRDROP else 0
            scaled_target = approx_target_in_zk(sigmas, Num(stake)) + headstart
            if stake>0:
                assert scaled_target>0
            return scaled_target

        apr = self.apr_scaled_to_runningtime()
        if (self.slot+1) % MIL_SLOT == 0:
            self.aprs += [apr]
        if self.slot % EPOCH_LENGTH ==0 and self.slot > 0:
            # staked ratio is added in strategy
            self.strategy.set_ratio(self.slot, apr)
            # epoch stake is added
            self.initial_stake += [self.stake]
        T = target(self.f, self.strategy.staked_value(self.stake))
        won, y = lottery(T, hp)
        self.won_hist += [won]
        return y, T
    """
    update stake upon winning lottery with single lead
    """
    def update_stake(self, reward):
        if self.won_hist[-1]:
            self.stake += reward

    """
    update stake after fork finalization
    """
    def resync_stake(self, reward):
        self.stake += reward


    def write(self, idx):
        with open('log/darkie'+str(idx)+'.log', 'w+') as f:
            buf = 'initial stake:'+','.join([str(i) for i in self.initial_stake])
            buf += '\r\n'
            buf += '(apr,staked ratio,{}):'.format(self.strategy.type)+','.join(['('+str(apr)+','+str(sr)+')' for sr, apr in zip(self.strategy.staked_tokens_ratio, self.strategy.annual_return)])
            buf+='\r\n'
            buf += 'apr: {}'.format(self.apr_scaled_to_runningtime())
            buf += '\r\n'
            buf += 'mil-aprs: {}'.format(','.join([str(apr) for apr in self.aprs]))
            buf += '\r\n'
            f.write(buf)

    """
    anonymous contract assumed to be random stream from uniform distribution,
    naive emulation of smart contract based transactions with certain computational cost.

    @returns: transaction emulated as series of random floats between 0,1
    """
    def tx(self, last_reward):
        tx_size = random.randint(0, MAX_BLOCK_SIZE)
        tip = self.tx_tip(tx_size, last_reward)
        if self.stake < tip:
            return Tx(tx_size, 0, self.idx)
        return Tx(tx_size, tip, self.idx)

    def tx_tip(self, tx_size, last_reward):
        tip = random_tip_strategy()
        apr = self.apr_scaled_to_runningtime()
        return tip.get_tip(float(last_reward), float(apr), tx_size, self.tips[-1])

    """
    deduct tip paid to miner plus burned base fee or computational cost.
    """
    def pay_fee(self, fee):
        if fee>0:
            self.fees += [fee]
        self.stake -= fee

    def last_fee(self):
        return self.fees[-1] if len(self.fees)>0 else 0

class Tx(object):
    def __init__(self, size, tip, idx):
        self.tx = [random.random() for _ in range(size)]
        self.len = size
        self.tip = tip
        self.idx=idx

    """
    anonymous contract assumed to be of random streams from uniform distribution,
    it's circuit execution cost it thus random.
    naive emulation of transaction smart contract computational cost as a avg of txs sum,
    which is random function

    @returns: transaction computational cost
    """
    def cc(self):
        return int(sum(self.tx) if len(self.tx)>0 else 0)

    def __len__(self):
        return len(self.tx)
