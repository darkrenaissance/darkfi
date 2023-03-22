import matplotlib.pyplot as plt
from tqdm import tqdm
from darkie import *
import time
from datetime import timedelta
from pid import PID

class DarkfiTable:
    def __init__(self, airdrop, running_time, controller_type=CONTROLLER_TYPE_DISCRETE, kp=0, ki=0, kd=0, dt=1, target=1, kc=0, ti=0, td=0, ts=0, debug=False):
        self.Sigma=airdrop
        self.darkies = []
        self.running_time=running_time
        self.start_time=None
        self.end_time=None
        self.pid = None
        self.pid = PID(kp=kp, ki=ki, kd=kd, dt=dt, target=target, Kc=kc, Ti=ti, Td=td, Ts=ts)
        self.controller_type=controller_type
        self.debug=debug

    def add_darkie(self, darkie):
        self.darkies+=[darkie]

    def background(self, rand_running_time=True, debug=False, hp=False):
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
            f = self.pid.pid_clipped(feedback, self.controller_type, debug)
            #note! thread overhead is 10X slower than sequential node execution!
            for i in range(len(self.darkies)):
                self.darkies[i].set_sigma_feedback(self.Sigma, feedback, f, count, hp)
                self.darkies[i].run(hp)
                total_vesting_stake+=self.darkies[i].update_vesting()
            self.Sigma+=total_vesting_stake
            for i in range(len(self.darkies)):
                winners += self.darkies[i].won
            feedback = winners
            count+=1
        self.end_time=time.time()
        return self.pid.acc()

    def write(self):
        elapsed=self.end_time-self.start_time
        if self.debug:
            print("total time: {}, slot time: {}".format(str(timedelta(seconds=elapsed)), str(timedelta(seconds=elapsed/self.running_time))))
        self.pid.write()
