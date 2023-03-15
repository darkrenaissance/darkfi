from lottery import *

AVG_LEN = 3

KC_STEP=0.1
KC_SEARCH_START=-2.3
KC_SEARCH_END=-1.9

TI_STEP=0.05
TI_SEARCH_START=-0.7
TI_SEARCH_END=-0.5

TD_STEP=0.05
TD_SEARCH_START=0.1
TD_SEARCH_END=0.3

TS_STEP=0.05
TS_SEARCH_START=-0.4
TS_SEARCH_END=-0.2

EPSILON=0.0001
RUNNING_TIME=1000

NODES=1000

randomize_nodes_str = input("randomize number of nodes (y/n):")
randomize_nodes = True if randomize_nodes_str.lower()=="y" else False

rand_running_time_str = input("random running time (y/n):")
rand_running_time = True if rand_running_time_str.lower()=="y" else False

debug_str = input("debug mode (y/n):")
debug = True if debug_str.lower()=="y" else False

target = 1
accuracy = []
# Kc
kc_range=tqdm(np.arange(KC_SEARCH_START, KC_SEARCH_END, KC_STEP))
for kc in kc_range:
    kc_range.set_description('kc: {}'.format(kc))
    if kc == 0:
        continue
    # Ti
    ti_range=tqdm(np.arange(TI_SEARCH_START, TI_SEARCH_END, TI_STEP))
    for ti in ti_range:
        ti_range.set_description('kc: {}, ti: {}'.format(kc, ti))
        if ti == 0:
            continue
        # Td
        td_range = tqdm(np.arange(TD_SEARCH_START, TD_SEARCH_END, TD_STEP))
        for td in td_range:
            td_range.set_description('kc: {}, ti: {}, td: {}'.format(kc, ti, td))
            if td == 0:
                continue
            # Ts
            ts_range = tqdm(np.arange(TS_SEARCH_START, TS_SEARCH_END, TS_STEP))
            for ts in ts_range:
                ts_range.set_description('kc: {}, ti: {}, td: {}, ts: {}'.format(kc, ti, td, ts))
                if ts == 0:
                    continue
                accs = []
                for i in range(0, AVG_LEN):
                    dt = DarkfiTable(0, RUNNING_TIME, kc=kc, ti=ti, ts=ts, td=td)
                    darkie_accs = []
                    #sum_airdrops = 0
                    # random nodes
                    RND_NODES = random.randint(5, NODES) if randomize_nodes else NODES
                    for idx in range(0,RND_NODES):
                        # random airdrops
                        #darkie_airdrop = None
                        #if idx == RND_NODES-1:
                            #darkie_airdrop = AIRDROP - sum_airdrops
                        #else:
                            #remaining_stake = (AIRDROP-RND_NODES)-sum_airdrops
                            #if remaining_stake <= 1:
                                #continue
                            #darkie_airdrop = random.randrange(1, remaining_stake)
                        #sum_airdrops += darkie_airdrop
                        darkie = Darkie(CONTROLLER_TYPE_TAKAHASHI)
                        dt.add_darkie(darkie)
                        darkie_acc = dt.background(rand_running_time, debug)
                        darkie_accs+=[darkie_acc]
                    acc = sum(darkie_accs)/(float(len(darkie_accs))+EPSILON)
                    accs+=[acc]
                avg_acc = sum(accs)/float(AVG_LEN)
                gains = (avg_acc, (kc, ti, td, ts))
                accuracy+=[gains]


accuracy=sorted(accuracy, key=lambda i: i[0], reverse=True)
with open("takahashi_gains.txt", "w") as f:
    buff=''
    for gain in accuracy:
        line=str(gain[0])+','+','.join([str(i) for i in gain[1]])+'\n'
        buff+=line
        f.write(buff)
