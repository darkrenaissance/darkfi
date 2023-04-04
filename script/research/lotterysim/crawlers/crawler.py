from lottery import *

AVG_LEN = 3

KP_STEP=0.01
KP_SEARCH=0.5

KI_STEP=0.01
KI_SEARCH=0.05

KD_STEP=0.01
KD_SEARCH=-0.36

EPSILON=0.0001
RUNNING_TIME=100

#AIRDROP=1000
NODES=500

highest_acc = 0


KP='kp'
KI='ki'
KD='kd'

crawl = KP
crawl_str = input("crawl (kp/ki/kd):")

if crawl_str == KI:
    crawl=KI
elif crawl_str == KD:
    crawl=KD


high_precision_str = input("high precision arith (slooow) (y/n):")
high_precision = True if high_precision_str.lower()=="y" else False


randomize_nodes_str = input("randomize number of nodes (y/n):")
randomize_nodes = True if randomize_nodes_str.lower()=="y" else False

rand_running_time_str = input("random running time (y/n):")
rand_running_time = True if rand_running_time_str.lower()=="y" else False

debug_str = input("debug mode (y/n):")
debug = True if debug_str.lower()=="y" else False



def experiment(accs=[], controller_type=CONTROLLER_TYPE_DISCRETE, kp=0, ki=0, kd=0, distribution=[], hp=False):
    dt = DarkfiTable(sum(distribution), RUNNING_TIME, controller_type, kp=kp, ki=ki, kd=kd)
    RND_NODES = random.randint(5, NODES) if randomize_nodes else NODES
    for idx in range(0,RND_NODES):
        darkie = Darkie(distribution[idx])
        dt.add_darkie(darkie)
    acc = dt.background(rand_running_time, hp)
    print('acc: {}'.format(acc))
    accs+=[acc]
    return acc


def multi_trial_exp(gains, kp, ki, kd, distribution = [], hp=False):
    global highest_acc
    accs = []
    for i in range(0, AVG_LEN):
        acc = experiment(accs, CONTROLLER_TYPE_DISCRETE, kp=kp, ki=ki, kd=kd, distribution=distribution, hp=hp)
        accs += [acc]

    avg_acc = sum(accs)/float(AVG_LEN)
    buff = 'accuracy:{}, kp: {}, ki:{}, kd:{}'.format(avg_acc, kp, ki, kd)
    print(buff)
    if avg_acc > 0:
        gain = (avg_acc, (kp, ki, kd))
        gains += [gain]
        if avg_acc > highest_acc:
            highest_acc = avg_acc
            with open("highest_gain.txt", 'w') as f:
                f.write(buff)

def single_trial_exp(gains, kp, ki, kd, distribution=[], hp=False):
    global highest_acc
    acc = experiment(kp=kp, ki=ki, kd=kd, distribution=distribution, hp=hp)
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
    crawl_range = None
    start = None
    if crawl==KP:
        start = KP_SEARCH
        step = KP_STEP
    elif crawl==KI:
        start = KI_SEARCH
        step = KI_STEP
    elif crawl==KD:
        start = KD_SEARCH
        step = KD_STEP
    step = 0.01
    rhs = np.arange(start, start*3, step) if start>=0 else  np.arange(start*3, start, step)
    lhs = np.flip(np.arange(-3*start, start, step)) if start<0 else np.flip(np.arange(start, -3*start, step))
    crawl_range=tqdm(np.concatenate((rhs, lhs)))
    distribution = [random.random() for i in range(NODES)]
    for i in crawl_range:
        crawl_range.set_description("crawling {} at {}".format(crawl, i))
        kp = i if crawl==KP else KP_SEARCH
        ki = i if crawl==KI else KI_SEARCH
        kd = i if crawl==KD else KD_SEARCH
        multi_trial_exp(gains, kp, ki, kd, distribution, hp=high_precision)

    gains=sorted(gains, key=lambda i: i[0], reverse=True)
    with open("gains.txt", "w") as f:
        buff=''
        for gain in gains:
            line=str(gain[0])+',' +','.join([str(i) for i in gain[1]])+'\n'
            buff+=line
            f.write(buff)
