import matplotlib.pyplot as plt
from tqdm import tqdm
import time
from datetime import timedelta
from core.darkie import *
from pid.cascade import *
from tqdm import tqdm
import random

class DarkfiTable:
    def __init__(self, airdrop, running_time, controller_type=CONTROLLER_TYPE_DISCRETE, kp=0, ki=0, kd=0, dt=1, kc=0, ti=0, td=0, ts=0, debug=False, r_kp=0, r_ki=0, r_kd=0, fee_kp=0, fee_ki=0, fee_kd=0):
        self.Sigma=airdrop
        self.darkies = []
        self.running_time=running_time
        self.start_time=None
        self.end_time=None
        self.secondary_pid = SecondaryDiscretePID(kp=kp, ki=ki, kd=kd) if controller_type==CONTROLLER_TYPE_DISCRETE else SecondaryTakahashiPID(kc=kc, ti=ti, td=td, ts=ts)
        print('secondary min/max : {}/{}'.format(self.secondary_pid.clip_min, self.secondary_pid.clip_max))
        self.primary_pid = PrimaryDiscretePID(kp=r_kp, ki=r_ki, kd=r_kd) if controller_type==CONTROLLER_TYPE_DISCRETE else PrimaryTakahashiPID(kc=kc, ti=ti, td=td, ts=ts)
        print('primary min/max : {}/{}'.format(self.primary_pid.clip_min, self.primary_pid.clip_max))
        self.basefee_pid = FeePID(kp=fee_kp, ki=fee_ki, kd=fee_kd)
        self.debug=debug
        self.rewards = []
        self.winners = [1]
        self.computational_cost = [0]
        self.base_fee = []
        self.tips_avg = []
        self.cc_diff = []

    def add_darkie(self, darkie):
        self.darkies+=[darkie]

    """
    for every slot under given running time, set f based off prior on-chain public \
    values, set sigmas, f, update vesting, stake for every stakeholder, resolve \
    forks.
    @param rand_running_time: randomization running time state
    @param debug: debug option
    @param hp: high precision option
    @returns: acc, avg_apy, avg_reward, stake_ratio, avg_apr
    """
    def background(self, rand_running_time=True, debug=False, hp=True):
        self.debug=debug
        self.start_time=time.time()
        # random running time
        rand_running_time = random.randint(1,self.running_time) if rand_running_time else self.running_time
        self.running_time = rand_running_time
        rt_range = tqdm(np.arange(0,self.running_time, 1))

        # loop through slots
        for slot in rt_range:
            # calculate probability of winning owning 100% of stake
            f = self.secondary_pid.pid_clipped(float(self.winners[-1]), debug)
            # calculate reward value every epoch
            if slot%EPOCH_LENGTH == 0:
                acc = self.secondary_pid.acc()
                reward = self.primary_pid.pid_clipped(acc, debug)
                self.rewards += [reward]
            #note! thread overhead is 10X slower than sequential node execution!
            total_stake = 0
            Ys = []
            Ts = []
            for i in range(len(self.darkies)):
                self.darkies[i].set_sigma_feedback(self.Sigma, self.winners[-1], f, slot, hp)
                self.darkies[i].update_vesting()
                y, T = self.darkies[i].run(hp)
                Ys+=[y]
                Ts+=[T]
                total_stake += self.darkies[i].stake
            # slot secondary controller feedback
            self.winners += [sum([self.darkies[i].won_hist[-1] for i in range(len(self.darkies))])]
            if self.winners[-1]==1:
                is_slashed = self.reward_slash_lead(debug)
                if is_slashed==False:
                    self.resolve_fork(slot, debug)
            avg_y = sum(Ys)/len(Ys)
            avg_t = sum(Ts)/len(Ts)
            avg_tip = self.tips_avg[-1] if len(self.tips_avg)>0 else 0
            base_fee = self.base_fee[-1] if len(self.base_fee)>0 else 0
            cc_diff = self.cc_diff[-1] if len(self.cc_diff)>0 else 0
            rt_range.set_description('epoch: {}, fork: {}, winners: {}, issuance {} DRK, f: {}, acc: {}%, stake: {}%, sr: {}%, reward:{}, apr: {}%, basefee: {}, avg(fee): {}, cc_diff: {}, avg(y): {}, avg(T): {}'.format(int(slot/EPOCH_LENGTH), self.merge_length(), self.winners[-1], round(self.Sigma,2), round(f, 5), round(acc*100, 2), round(total_stake/self.Sigma*100 if self.Sigma>0 else 0,2), round(self.avg_stake_ratio()*100,2) , round(self.rewards[-1],2), round(self.avg_apr()*100,2), round(base_fee, 4),  round(avg_tip, 2), round(cc_diff, 2), round(float(avg_y), 2), round(float(avg_t), 2)))
            #assert round(total_stake,1) <= round(self.Sigma,1), 'stake: {}, sigma: {}'.format(total_stake, self.Sigma)
            slot+=1
        self.end_time=time.time()
        avg_reward = sum(self.rewards)/len(self.rewards)
        stake_ratio = self.avg_stake_ratio()
        avg_apy = self.avg_apy()
        avg_apr = self.avg_apr()
        cc_diff_avg = sum([0 if math.fabs(i)<CC_DIFF_EPSILON else 1 for i in self.cc_diff])/len(self.cc_diff) if len(self.cc_diff)>0 else 0
        return self.secondary_pid.acc_percentage(), cc_diff_avg, avg_apy, avg_reward, stake_ratio, avg_apr

    """
    reward single lead, or slash lead with probability len(self.darkies)**-1

    @returns: True if slashed False otherwise
    """
    def reward_slash_lead(self, debug=False):
        # reward the single lead
        for i in range(len(self.darkies)):
            if self.darkies[i].won_hist[-1]:
                if random.random() < len(self.darkies)**-1:
                    self.darkies.remove(self.darkies[i])
                    print('stakeholder {} slashed'.format(i))
                    return True
                else:
                    self.darkies[i].update_stake(self.rewards[-1])
                    self.Sigma += self.rewards[-1]
                    self.tx_fees(i, debug)
                break
        return False

    """
    resolve fork, for slots with multiple leads, shuffle nodes, and reward first winner.
    """
    def resolve_fork(self, slot, debug=False):
        # resolve fork
        for i in range(self.merge_length()):
            resync_slot_id = slot-(i+1)
            resync_reward_id = int((resync_slot_id)/EPOCH_LENGTH)
            resync_reward = self.rewards[resync_reward_id]
            # resyncing depends on the random branch chosen,
            # it's simulated by choosing first wining node
            darkie_winning_idx = -1
            random.shuffle(self.darkies)
            for darkie_idx in range(len(self.darkies)):
                if self.darkies[darkie_idx].won_hist[resync_slot_id]:
                    self.darkies[darkie_idx].resync_stake(resync_reward)
                    self.Sigma += resync_reward

    def merge_length(self):
        merge_length = 0
        for i in reversed(self.winners[:-1]):
            if i !=1:
                merge_length+=1
            else:
                break
        return merge_length

    """
    simulate general purpose transactions made by stakeholders,
    deduct basefee, tip from senders pay miners tipss.
    """
    def tx_fees(self, darkie_lead_idx, debug=False):
        txs = []
        for darkie in self.darkies:
            # make sure tip is covered by darkie stake
            txs += [darkie.tx(self.rewards[-1])]
        ret, actual_cc = DarkfiTable.auction(txs)
        self.computational_cost += [actual_cc]
        self.cc_diff += [MAX_BLOCK_CC - actual_cc]
        tips = ret[0]
        idxs = ret[1]
        self.tips_avg += [tips/len(idxs) if len(idxs)>0 else 0]
        basefee = self.basefee_pid.pid_clipped(self.computational_cost[-1], debug)
        self.base_fee+=[basefee]
        for idx in idxs:
            fee = txs[idx].cc()+basefee
            self.darkies[idx].pay_fee(fee)
            #print("charging darkie[{}]: {} DRK per tx of length: {}, burning: {}".format(idx, fee, len(txs[idx]), basefee))
        self.darkies[darkie_lead_idx].pay_fee(-1*tips)

        # subtract base fee from total stake
        self.Sigma -= basefee*len(txs)

    """
    average APY (with compound interest added every epoch) ,
    scaled to running time for all nodes
    @returns: average APY for all nodes
    """
    def avg_apy(self):
        return Num(sum([darkie.apy_scaled_to_runningtime(self.rewards) for darkie in self.darkies])/len(self.darkies))

    """
    average APR scaled to running time for all nodes
    @returns: average APR for all nodes
    """
    def avg_apr(self):
        return Num(sum([darkie.apr_scaled_to_runningtime() for darkie in self.darkies])/len(self.darkies))

    """
    returns: average stake ratio for all nodes
    """
    def avg_stake_ratio(self):
        return sum([darkie.staked_tokens_ratio() for darkie in self.darkies]) / len(self.darkies)

    """
    write lottery reward log
    """
    def write(self):
        elapsed=self.end_time-self.start_time
        for id, darkie in enumerate(self.darkies):
            darkie.write(id)
        if self.debug:
            print("total time: {}, slot time: {}".format(str(timedelta(seconds=elapsed)), str(timedelta(seconds=elapsed/self.running_time))))
        self.secondary_pid.write()
        with open('log/rewards.log', 'w+') as f:
            buff = ','.join([str(i) for i in self.rewards])
            f.write(buff)

    """
    tip auction

    @return total tip for miner, and list of indices of darkies included.
    """
    def auction(txs):
        W = MAX_BLOCK_CC
        n = len(txs)
        K = [[[0,[]] for x in range(W + 1)] for x in range(n + 1)]
        for i in range(n + 1):
            for w in range(W + 1):
                if i == 0 or w == 0:
                    K[i][w] = [0,[]]
                elif txs[i-1].cc() <= w:
                    if txs[i-1].tip + K[i-1][w-txs[i-1].cc()][0] > K[i-1][w][0]:
                        K[i][w] = [txs[i-1].tip + K[i-1][w-txs[i-1].cc()][0], K[i-1][w-txs[i-1].cc()][1] + [i-1]]
                    else:
                        K[i][w] = K[i-1][w]
                else:
                    K[i][w] = K[i-1][w]
        tip = K[n][W][0]
        actual_cc = W
        for w in reversed(range(W+1)):
            if K[n][w][0] == tip:
                actual_cc = w
            else:
                break
        return K[n][W], actual_cc
