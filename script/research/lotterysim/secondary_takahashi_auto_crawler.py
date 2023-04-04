from argparse import ArgumentParser
from core.lottery import DarkfiTable
from core.utils import *
from core.darkie import Darkie
from tqdm import tqdm
from core.strategy import SigmoidStrategy
import os

AVG_LEN = 5

KC_STEP=0.1
KC_SEARCH=-0.5129999999999987

TD_STEP=0.01
TD_SEARCH=0.2690000000000005

TI_STEP=0.01
TI_SEARCH=0.004000000000058401

TS_STEP=0.01
TS_SEARCH=-1.4560000000001243

EPSILON=0.0001
RUNNING_TIME=1000
NODES=1000

highest_acc = 0

KC='KC'
TI='TI'
TD='TD'
TS='TS'

KC_RANGE_MULTIPLIER = 2
TI_RANGE_MULTIPLIER = 2
TD_RANGE_MULTIPLIER = 2
TS_RANGE_MULTIPLIER = 2

highest_gain = (KC_SEARCH, TI_SEARCH, TD_SEARCH, TS_SEARCH)

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


def experiment(controller_type=CONTROLLER_TYPE_TAKAHASHI, kp=0, ki=0, kd=0, kc=0, ti=0, td=0, ts=0, distribution=[], hp=False):
    dt = DarkfiTable(ERC20DRK, RUNNING_TIME, controller_type, kp=kp, ki=ki, kd=kd, kc=kc, td=td, ti=ti, ts=ts)
    RND_NODES = random.randint(5, NODES) if randomize_nodes else NODES
    for idx in range(0,RND_NODES):
        darkie = Darkie(distribution[idx], strategy=SigmoidStrategy(EPOCH_LENGTH), apy_window=EPOCH_LENGTH)
        dt.add_darkie(darkie)
    acc, apy, reward, stake_ratio = dt.background_with_apy(rand_running_time, hp)
    return acc


def multi_trial_exp(kc, td, ti, ts, distribution = [], hp=False):
    global highest_acc
    global highest_gain
    new_record = False
    accs = []
    for i in range(0, AVG_LEN):
        acc = experiment(CONTROLLER_TYPE_DISCRETE, kc=kc, ti=ti, td=td, ts=ts, distribution=distribution, hp=hp)
        accs += [acc]
    avg_acc = sum(accs)/float(AVG_LEN)
    buff = 'accuracy:{}, kc: {}, td:{}, ti:{}, ts:{}'.format(avg_acc, kc, td, ti, ts)
    if avg_acc > 0:
        gain = (kc, td, ti, ts)
        acc_gain = (avg_acc, gain)
        if avg_acc > highest_acc:
            new_record = True
            highest_acc = avg_acc
            highest_gain = gain
            with open('log'+os.sep+"highest_gain_takahashi.txt", 'w') as f:
                f.write(buff)
    return buff, new_record

SHIFTING = 0.05

def crawler(crawl, range_multiplier, step=0.1):
    start = None
    if crawl==KC:
        start = highest_gain[0]
    elif crawl==TI:
        start = highest_gain[1]
    elif crawl==TD:
        start = highest_gain[2]
    elif crawl==TS:
        start = highest_gain[3]
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
    distribution = [random.random()*ERC20DRK*0.0001 for i in range(NODES)]
    for i in crawl_range:
        kc = i if crawl==KC else highest_gain[0]
        ti = i if crawl==TI else highest_gain[1]
        td = i if crawl==TD else highest_gain[2]
        ts = i if crawl==TS else highest_gain[3]
        buff, new_record = multi_trial_exp(kc, td, ti, ts, distribution, hp=high_precision)
        crawl_range.set_description('highest:{} / {}'.format(highest_acc, buff))
        if new_record:
            break

while True:
    prev_highest_gain = highest_gain
    # kc crawl
    crawler(KC, KC_RANGE_MULTIPLIER, KC_STEP)
    if highest_gain[0] == prev_highest_gain[0]:
        KC_RANGE_MULTIPLIER+=1
        KC_STEP/=10
    else:
        start = highest_gain[0]
        range_start = (start*KC_RANGE_MULTIPLIER if start <=0 else -1*start) - SHIFTING
        range_end = (-1*start if start<=0 else KC_RANGE_MULTIPLIER*start) + SHIFTING
        while (range_end - range_start)/KC_STEP >500:
            if KC_STEP < 0.1:
                KC_STEP*=10
            KC_RANGE_MULTIPLIER-=1
            #TODO (res) shouldn't the range also shrink?
            # not always true.
            # how to distinguish between thrinking range, and large step?
            # good strategy is step shoudn't > 0.1
            # range also should be > 0.8
            # what about range multiplier?

    # td crawl
    crawler(TD, TD_RANGE_MULTIPLIER, TD_STEP)
    if highest_gain[2] == prev_highest_gain[2]:
        TD_RANGE_MULTIPLIER+=1
        TD_STEP/=10
    else:
        start = highest_gain[2]
        range_start = (start*TD_RANGE_MULTIPLIER if start <=0 else -1*start) - SHIFTING
        range_end = (-1*start if start<=0 else TD_RANGE_MULTIPLIER*start) + SHIFTING
        while (range_end - range_start)/TD_STEP >500:
            if TD_STEP < 0.1:
                TD_STEP*=10
            TD_RANGE_MULTIPLIER-=1

    # ti crawl
    crawler(TI, TI_RANGE_MULTIPLIER, TI_STEP)
    if highest_gain[1] == prev_highest_gain[1]:
        TI_RANGE_MULTIPLIER+=1
        TI_STEP/=10
    else:
        start = highest_gain[1]
        range_start = (start*TI_RANGE_MULTIPLIER if start <=0 else -1*start) - SHIFTING
        range_end = (-1*start if start<=0 else TI_RANGE_MULTIPLIER*start) + SHIFTING
        while (range_end - range_start)/TI_STEP >500:
            if TP_STEP < 0.3:
                TI_STEP*=10
            TI_RANGE_MULTIPLIER-=1

    # tS crawl
    crawler(TS, TS_RANGE_MULTIPLIER, TS_STEP)
    if highest_gain[2] == prev_highest_gain[2]:
        TS_RANGE_MULTIPLIER+=1
        TS_STEP/=10
    else:
        start = highest_gain[2]
        range_start = (start*TS_RANGE_MULTIPLIER if start <=0 else -1*start) - SHIFTING
        range_end = (-1*start if start<=0 else TS_RANGE_MULTIPLIER*start) + SHIFTING
        while (range_end - range_start)/TS_STEP >500:
            if TS_STEP < 0.1:
                TS_STEP*=10
            TS_RANGE_MULTIPLIER-=1
