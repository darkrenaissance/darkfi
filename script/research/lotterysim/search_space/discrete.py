/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

from lottery import *
from threading import Thread

AVG_LEN = 3

KP_STEP=0.05
KP_SEARCH_START=-0.1
KP_SEARCH_END=0.3

KI_STEP=0.05
KI_SEARCH_START=-0.1
KI_SEARCH_END=0.1

KD_STEP=0.05
KD_SEARCH_START=-0.2
KD_SEARCH_END=0.2

EPSILON=0.0001
RUNNING_TIME=1000

#AIRDROP=1000
NODES=1000

high_precision_str = input("high precision arith (slooow) (y/n):")
high_precision = True if high_precision_str.lower()=="y" else False


randomize_nodes_str = input("randomize number of nodes (y/n):")
randomize_nodes = True if randomize_nodes_str.lower()=="y" else False

rand_running_time_str = input("random running time (y/n):")
rand_running_time = True if rand_running_time_str.lower()=="y" else False

debug_str = input("debug mode (y/n):")
debug = True if debug_str.lower()=="y" else False


def experiment(accs=[], controller_type=CONTROLLER_TYPE_DISCRETE, kp=0, ki=0, kd=0, airdrop=0, hp=False):
    dt = DarkfiTable(ERC20DRK, RUNNING_TIME, controller_type, kp=kp, ki=ki, kd=kd)
    RND_NODES = random.randint(5, NODES) if randomize_nodes else NODES
    for idx in range(0,RND_NODES):
        darkie = Darkie(random.random()*ERC20DRK/(RND_NODES))
        dt.add_darkie(darkie)
    acc = dt.background(rand_running_time, hp)
    accs+=[acc]
    return acc

highest_acc = 0

def multi_trial_exp(gains, kp, ki, kd, hp=False):
    global highest_acc
    experiment_accs = []
    exp_threads = []
    for i in range(0, AVG_LEN):
        experiment(experiment_accs, CONTROLLER_TYPE_DISCRETE, kp=kp, ki=ki, kd=kd, hp=hp)
        #exp_thread = Thread(target=experiment, args=[experiment_accs, CONTROLLER_TYPE_DISCRETE, kp, ki, kd])
        #exp_thread.start()
    #for thread in exp_threads:
        #thread.join()
    avg_acc = sum(experiment_accs)/float(AVG_LEN)
    buff = 'accuracy:{}, kp: {}, ki:{}, kd:{}'.format(avg_acc, kp, ki, kd)
    print(buff)
    if avg_acc > 0:
        gain = (avg_acc, (kp, ki, kd))
        gains += [gain]
        if avg_acc > highest_acc:
            highest_acc = avg_acc
            with open("highest_gain.txt", 'w') as f:
                f.write(buff)

def single_trial_exp(gains, kp, ki, kd, hp=False):
    global highest_acc
    acc = experiment(kp=kp, ki=ki, kd=kd, hp=hp)
    buff = 'accuracy:{}, kp: {}, ki:{}, kd:{}'.format(acc, kp, ki, kd)
    print(buff)
    if acc > 0:
        gain = (acc, (kp, ki, kd))
        gains += [gain]
        if acc > highest_acc:
            highest_acc = acc
            with open("highest_gain.txt", 'w') as f:
                f.write(buff)
        gains += [gain]


gains = []
if __name__ == "__main__":
    # kp
    gains_threads = []
    ki_range = tqdm(np.arange(KI_SEARCH_START, KI_SEARCH_END, KI_STEP))
    kd_range = tqdm(np.arange(KD_SEARCH_START, KD_SEARCH_END, KD_STEP))
    kp_range = tqdm(np.arange(KP_SEARCH_START, KP_SEARCH_END, KP_STEP))
    for kp in kp_range:
        kp_range.set_description('kp: {}'.format(kp))
        # ki
        for ki in ki_range:
            ki_range.set_description('kp: {}, ki: {}'.format(kp, ki))
            # kd
            for kd in kd_range:
                kd_range.set_description('kp: {}, ki: {}, kd: {}'.format(kp, ki, kd))
                multi_trial_exp(gains, kp, ki, kd, hp=high_precision)
                #thread = Thread(target=single_trial_exp, args=[gains, kp, ki, kd])
                #thread.start()
                #gains_threads += [thread]
    #for th in tqdm(gains_threads):
        #th.join()
    gains=sorted(gains, key=lambda i: i[0], reverse=True)
    with open("gains.txt", "w") as f:
        buff=''
        for gain in gains:
            line=str(gain[0])+',' +','.join([str(i) for i in gain[1]])+'\n'
            buff+=line
            f.write(buff)
