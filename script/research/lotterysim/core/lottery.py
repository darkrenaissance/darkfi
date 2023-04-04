import matplotlib.pyplot as plt
from tqdm import tqdm
import time
from datetime import timedelta
from core.darkie import *
from pid.cascade import *

class DarkfiTable:
    def __init__(self, airdrop, running_time, controller_type=CONTROLLER_TYPE_DISCRETE, kp=0, ki=0, kd=0, dt=1, kc=0, ti=0, td=0, ts=0, debug=False, r_kp=0, r_ki=0, r_kd=0):
        self.Sigma=airdrop
        self.darkies = []
        self.running_time=running_time
        self.start_time=None
        self.end_time=None
        self.secondary_pid = SecondaryDiscretePID(kp=kp, ki=ki, kd=kd) if controller_type==CONTROLLER_TYPE_DISCRETE else SecondaryTakahashiPID(kc=kc, ti=ti, td=td, ts=ts)
        self.primary_pid = PrimaryDiscretePID(kp=r_kp, ki=r_ki, kd=r_kd) if controller_type==CONTROLLER_TYPE_DISCRETE else PrimaryTakahashiPID(kc=kc, ti=ti, td=td, ts=ts)
        self.debug=debug
        self.rewards = [0]

    def add_darkie(self, darkie):
        self.darkies+=[darkie]

    def background(self, rand_running_time=True, debug=False, hp=True):
        self.debug=debug
        self.start_time=time.time()
        feedback=0 # number leads in previous slot
        count = 0
        # random running time
        rand_running_time = random.randint(1,self.running_time) if rand_running_time else self.running_time
        self.running_time = rand_running_time
        #if rand_running_time and debug:
            #print("random running time: {}".format(self.running_time))
            #print('running time: {}'.format(self.running_time))
        while count < self.running_time:
            winners=0
            total_vesting_stake = 0
            f = self.secondary_pid.pid_clipped(float(feedback),  debug)
            #note! thread overhead is 10X slower than sequential node execution!
            for i in range(len(self.darkies)):
                self.darkies[i].set_sigma_feedback(self.Sigma, feedback, f, count, hp)
                self.darkies[i].run(self.rewards, hp)
                total_vesting_stake+=self.darkies[i].update_vesting()
            for i in range(len(self.darkies)):
                winners += self.darkies[i].won
                print('reward: {}'.format(self.rewards[-1]))
                self.darkies[i].update_stake(self.rewards[-1])

            if count%EPOCH_LENGTH == 0:
                acc = self.secondary_pid.acc()
                reward = self.primary_pid.pid_clipped(float(self.avg_apy()), debug)
                self.rewards += [reward]
            feedback = winners
            if winners==1:
                if count >= ERC20DRK:
                    self.Sigma += 1
                for i in range(len(self.darkies)):
                    self.darkies[i].finalize_stake()
            count+=1
        self.end_time=time.time()
        return self.secondary_pid.acc()

    def background_with_apy(self, rand_running_time=True, debug=False, hp=True):
        self.debug=debug
        self.start_time=time.time()
        feedback=0 # number leads in previous slot
        count = 0
        # random running time
        rand_running_time = random.randint(1,self.running_time) if rand_running_time else self.running_time
        self.running_time = rand_running_time
        #if rand_running_time and debug:
            #print("random running time: {}".format(self.running_time))
            #print('running time: {}'.format(self.running_time))

        while count < self.running_time:
            winners=0
            total_vesting_stake = 0
            f = self.secondary_pid.pid_clipped(float(feedback), debug)

            #note! thread overhead is 10X slower than sequential node execution!
            for i in range(len(self.darkies)):
                self.darkies[i].set_sigma_feedback(self.Sigma, feedback, f, count, hp)
                self.darkies[i].run(self.rewards, hp)
                total_vesting_stake+=self.darkies[i].update_vesting()

            #print('reward: {}'.format(rewards[-1]))
            for i in range(len(self.darkies)):
                winners += self.darkies[i].won
                self.darkies[i].update_stake(self.rewards[-1])
                ###

            if count%EPOCH_LENGTH == 0 and count > EPOCH_LENGTH:
                acc = self.secondary_pid.acc_percentage()
                reward = self.primary_pid.pid_clipped(float(self.avg_stake_ratio()), debug)
                self.rewards += [reward]

            feedback = winners
            if winners==1:
                if count >= ERC20DRK:
                    self.Sigma += 1
                for i in range(len(self.darkies)):
                    self.darkies[i].finalize_stake()
            count+=1
        self.end_time=time.time()
        avg_reward = sum(self.rewards)/len(self.rewards)
        stake_ratio = self.avg_stake_ratio()
        avg_apy = self.avg_apy()
        #print('apy: {}, staked_ratio: {}'.format(avg_apy, stake_ratio))
        return self.secondary_pid.acc(), avg_apy, avg_reward, stake_ratio

    def avg_apy(self):
        return sum([darkie.apy_percentage(self.rewards) for darkie in self.darkies])/len(self.darkies)

    def avg_stake_ratio(self):
        return sum([darkie.staked_tokens_ratio() for darkie in self.darkies])/len(self.darkies)*100

    def write(self):
        elapsed=self.end_time-self.start_time
        if self.debug:
            print("total time: {}, slot time: {}".format(str(timedelta(seconds=elapsed)), str(timedelta(seconds=elapsed/self.running_time))))
        self.secondary_pid.write()
