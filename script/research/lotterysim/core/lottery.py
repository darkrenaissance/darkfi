import matplotlib.pyplot as plt
from tqdm import tqdm
import time
from datetime import timedelta
from core.darkie import *
from pid.cascade import *
from tqdm import tqdm
import random

class DarkfiTable:
    def __init__(self, airdrop, running_time, controller_type=CONTROLLER_TYPE_DISCRETE, kp=0, ki=0, kd=0, dt=1, kc=0, ti=0, td=0, ts=0, debug=False, r_kp=0, r_ki=0, r_kd=0):
        self.Sigma=airdrop
        self.darkies = []
        self.running_time=running_time
        self.start_time=None
        self.end_time=None
        self.secondary_pid = SecondaryDiscretePID(kp=kp, ki=ki, kd=kd) if controller_type==CONTROLLER_TYPE_DISCRETE else SecondaryTakahashiPID(kc=kc, ti=ti, td=td, ts=ts)
        print('secondary min/max : {}/{}'.format(self.secondary_pid.clip_min, self.secondary_pid.clip_max))
        self.primary_pid = PrimaryDiscretePID(kp=r_kp, ki=r_ki, kd=r_kd) if controller_type==CONTROLLER_TYPE_DISCRETE else PrimaryTakahashiPID(kc=kc, ti=ti, td=td, ts=ts)
        print('primary min/max : {}/{}'.format(self.primary_pid.clip_min, self.primary_pid.clip_max))
        self.debug=debug
        self.rewards = []
        self.winners = []

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
        feedback=0 # number leads in previous slot
        # random running time
        rand_running_time = random.randint(1,self.running_time) if rand_running_time else self.running_time
        self.running_time = rand_running_time
        rt_range = tqdm(np.arange(0,self.running_time, 1))
        merge_length = 0
        # loop through slots
        for count in rt_range:
            merge_length = 0
            # calculate probability of winning owning 100% of stake
            f = self.secondary_pid.pid_clipped(float(feedback), debug)
            # calculate reward value every epoch
            if count%EPOCH_LENGTH == 0:
                acc = self.secondary_pid.acc()
                reward = self.primary_pid.pid_clipped(acc, debug)
                self.rewards += [reward]
            #note! thread overhead is 10X slower than sequential node execution!
            total_stake = 0
            for i in range(len(self.darkies)):
                self.darkies[i].set_sigma_feedback(self.Sigma, feedback, f, count, hp)
                self.darkies[i].update_vesting()
                self.darkies[i].run(hp)
                total_stake += self.darkies[i].stake
            # count number of leads per slot
            winners=0
            # count secondary controller feedback
            for i in range(len(self.darkies)):
                winners += self.darkies[i].won_hist[-1]
            self.winners +=[winners]
            feedback = winners
            ################
            # resolve fork #
            ################
            if self.winners[-1]==1:
                for i in range(len(self.darkies)):
                    if self.darkies[i].won_hist[-1]:
                        if random.random() < len(self.darkies)**-1:
                            self.darkies.remove(self.darkies[i])
                            print('stakeholder {} slashed'.format(i))
                        else:
                            self.darkies[i].update_stake(self.rewards[-1])
                        break
                # resolve finalization
                self.Sigma += self.rewards[-1]
                # resync nodes

                for i in reversed(self.winners[:-1]):
                    if i !=1:
                        merge_length+=1
                    else:
                        break
                for i in range(merge_length):
                    resync_slot_id = count-(i+1)
                    resync_reward_id = int((resync_slot_id)/EPOCH_LENGTH)
                    resync_reward = self.rewards[resync_reward_id]
                    # resyncing depends on the random branch chosen,
                    # it's simulated by choosing first wining node
                    darkie_winning_idx = -1
                    random.shuffle(self.darkies)
                    for darkie_idx in range(len(self.darkies)):
                        if self.darkies[darkie_idx].won_hist[resync_slot_id]:
                            darkie_winning_idx = darkie_idx
                            break
                    if self.darkie_winning_idx>=0:
                        self.darkies[darkie_winning_idx].resync_stake(resync_reward)
                        self.Sigma += resync_reward
                    else:
                        # single lead got slashed
                        pass
            #################
            # fork resolved #
            #################
            rt_range.set_description('epoch: {}, fork: {} issuance {} DRK, acc: {}%, stake = {}%, sr: {}%, reward:{}, apr: {}%'.format(int(count/EPOCH_LENGTH), merge_length, round(self.Sigma,2), round(acc*100, 2), round(total_stake/self.Sigma*100 if self.Sigma>0 else 0,2), round(self.avg_stake_ratio()*100,2) , round(self.rewards[-1],2), round(self.avg_apr()*100,2)))
            #assert round(total_stake,1) <= round(self.Sigma,1), 'stake: {}, sigma: {}'.format(total_stake, self.Sigma)
            count+=1
        self.end_time=time.time()
        avg_reward = sum(self.rewards)/len(self.rewards)
        stake_ratio = self.avg_stake_ratio()
        avg_apy = self.avg_apy()
        avg_apr = self.avg_apr()
        return self.secondary_pid.acc_percentage(), avg_apy, avg_reward, stake_ratio, avg_apr

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
