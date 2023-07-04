from argparse import ArgumentParser
from core.lottery import DarkfiTable
from core.utils import *
from core.darkie import Darkie
from tqdm import tqdm
from core.strategy import SigmoidStrategy
import os

AVG_LEN = 5

KP_STEP=0.01
KP_SEARCH= -0.01

KI_STEP=0.01
KI_SEARCH=-0.036

EPSILON=0.0001
RUNNING_TIME=1000
NODES = 1000

highest_acc = 0

KP='kp'
KI='ki'

KP_RANGE_MULTIPLIER = 2
KI_RANGE_MULTIPLIER = 2

highest_gain = (KP_SEARCH, KI_SEARCH)

parser = ArgumentParser()
parser.add_argument('-p', '--high-precision', action='store_true')
parser.add_argument('-r', '--randomize-nodes', action='store_false')
parser.add_argument('-t', '--rand-running-time', action='store_false')
parser.add_argument('-d', '--debug', action='store_false')
args = parser.parse_args()
high_precision = args.high_precision
randomize_nodes = args.randomize_nodes
rand_running_time = args.rand_running_time
debug = args.debug

def experiment(controller_type=CONTROLLER_TYPE_DISCRETE, kp=0, ki=0, distribution=[], hp=True):
    dt = DarkfiTable(ERC20DRK, RUNNING_TIME, controller_type, kp=kp, ki=ki, kd=0)
    RND_NODES = random.randint(5, NODES) if randomize_nodes else NODES
    for idx in range(0,RND_NODES):
        darkie = Darkie(distribution[idx], strategy=SigmoidStrategy(EPOCH_LENGTH))
        dt.add_darkie(darkie)
    acc, apy, reward, stake_ratio, apr = dt.background(rand_running_time, hp)
    return acc

def multi_trial_exp(kp, ki, distribution = [], hp=True):
    global highest_acc
    global highest_gain
    new_record=False
    exp_threads = []
    accs = []
    for i in range(0, AVG_LEN):
        acc = experiment(CONTROLLER_TYPE_DISCRETE, kp=kp, ki=ki, distribution=distribution, hp=hp)
        accs += [acc]
    avg_acc = sum(accs)/float(AVG_LEN)
    buff = 'accuracy:{}, kp: {}, ki:{}'.format(avg_acc, kp, ki)
    if avg_acc > 0:
        gain = (kp, ki)
        acc_gain = (avg_acc, gain)
        if avg_acc > highest_acc:
            new_record = True
            highest_acc = avg_acc
            highest_gain = (kp, ki)
            with open('log'+os.sep+"highest_gain.txt", 'w') as f:
                f.write(buff)
    return buff, new_record

SHIFTING = 0.05

def crawler(crawl, range_multiplier, step=0.1):
    start = None
    if crawl==KP:
        start = highest_gain[0]
    elif crawl==KI:
        start = highest_gain[1]

    range_start = (start*range_multiplier if start <=0 else -1*start)
    range_end = (-1*start if start<=0 else range_multiplier*start)
    # if number of steps under 10 step resize the step to 50
    while (range_end-range_start)/step < 10:
        range_start -= SHIFTING
        range_end += SHIFTING
        step /= 10

    crawl_range = np.arange(range_start, range_end, step)
    np.random.shuffle(crawl_range)
    crawl_range = tqdm(crawl_range)
    distribution = [random.gauss(ERC20DRK/NODES, ERC20DRK/NODES*0.1) for i in range(NODES)]
    for i in crawl_range:
        kp = i if crawl==KP else highest_gain[0]
        ki = i if crawl==KI else highest_gain[1]
        buff, new_record = multi_trial_exp(kp, ki, distribution, hp=high_precision)
        crawl_range.set_description('highest:{} / {}'.format(highest_acc, buff))
        if new_record:
            break

while True:
    prev_highest_gain = highest_gain
    # kp crawl
    crawler(KP, KP_RANGE_MULTIPLIER, KP_STEP)
    if highest_gain[0] == prev_highest_gain[0]:
        KP_RANGE_MULTIPLIER+=1
        KP_STEP/=10
    else:
        start = highest_gain[0]
        range_start = (start*KP_RANGE_MULTIPLIER if start <=0 else -1*start) - SHIFTING
        range_end = (-1*start if start<=0 else KP_RANGE_MULTIPLIER*start) + SHIFTING
        while (range_end - range_start)/KP_STEP >500:
            if KP_STEP < 0.1:
                KP_STEP*=10
            KP_RANGE_MULTIPLIER-=1
            #TODO (res) shouldn't the range also shrink?
            # not always true.
            # how to distinguish between thrinking range, and large step?
            # good strategy is step shoudn't > 0.1
            # range also should be > 0.8
            # what about range multiplier?

    # ki crawl
    crawler(KI, KI_RANGE_MULTIPLIER, KI_STEP)
    if highest_gain[1] == prev_highest_gain[1]:
        KI_RANGE_MULTIPLIER+=1
        KI_STEP/=10
    else:
        start = highest_gain[1]
        range_start = (start*KI_RANGE_MULTIPLIER if start <=0 else -1*start) - SHIFTING
        range_end = (-1*start if start<=0 else KI_RANGE_MULTIPLIER*start) + SHIFTING
        while (range_end - range_start)/KI_STEP >500:
            if KP_STEP < 0.1:
                KI_STEP*=10
            KI_RANGE_MULTIPLIER-=1
