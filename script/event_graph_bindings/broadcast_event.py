from darkfi_eventgraph_py import event_graph as eg, p2p, sled
import asyncio
import random
import time
import subprocess
import os
import threading
# number of nodes
N = 3
P2PDATASTORE_PATH = '/tmp/p2pdatastore'
SLED_DB_PATH = '/tmp/sleddb'
STARTING_PORT = 54321
os.system("rm -rf " + P2PDATASTORE_PATH+"*")
os.system("rm -rf " + SLED_DB_PATH+"*")


async def get_greylist_length(node):
    return await p2p.get_greylist_length(node)

def get_random_node_idx():
    return int(random.random()*(N-1))

async def start_p2p(w8_time, node):
    await p2p.start_p2p(w8_time, node)

async def get_fut_p2p(settings):
    return await p2p.new_p2p(settings)

async def get_fut_eg(node, sled_db):
    return await eg.new_event_graph(node, sled_db, P2PDATASTORE_PATH, False, 'dag', 1)

async def register_protocol(p2p_node, eg_node):
    await p2p.register_protocol_p2p(p2p_node, eg_node)
# create p2p node
def get_seed_node(starting_port=STARTING_PORT):
    inbound_port = starting_port
    node_id = str(inbound_port)
    seed_addr = p2p.Url("tcp://127.0.0.1:{}".format(inbound_port))
    inbound_addrs = [seed_addr]
    external_addrs = []
    peers = []
    seeds = []
    app_version = p2p.new_version(0, 1, 1, '')
    allowed_transports = ['tcp']
    transport_mixing = True
    outbound_connections = 0
    inbound_connections = 1000
    outbound_connect_timeout = 15
    channel_handshake_timeout = 15
    channel_heartbeat_interval = 30
    localnet = True
    outbound_peer_discovery_cooloff_time = 30
    outbound_peer_discovery_attempt_time = 5
    p2p_datastore = P2PDATASTORE_PATH+'{}'.format(0)
    hostlist = ''
    greylist_refinery_internval = 15
    white_connect_percnet = 70
    gold_connect_count = 2
    slot_preference_strict = False
    time_with_no_connections = 30
    blacklist = []
    ban_policy = p2p.get_strict_banpolicy()
    settings = p2p.new_settings(
        node_id,
        inbound_addrs,
        external_addrs,
        peers,
        seeds,
        app_version,
        allowed_transports,
        transport_mixing,
        outbound_connections,
        inbound_connections,
        outbound_connect_timeout,
        channel_handshake_timeout,
        channel_heartbeat_interval,
        localnet,
        outbound_peer_discovery_cooloff_time,
        outbound_peer_discovery_attempt_time,
        p2p_datastore,
        hostlist,
        greylist_refinery_internval,
        white_connect_percnet,
        gold_connect_count,
        slot_preference_strict,
        time_with_no_connections,
        blacklist,
        ban_policy
    )
    seed_p2p_ptr = asyncio.run(get_fut_p2p(settings))
    seed_sled_db = sled.SledDb(SLED_DB_PATH+'{}'.format(0))
    seed_event_graph = asyncio.run(get_fut_eg(seed_p2p_ptr, seed_sled_db))
    return seed_p2p_ptr, seed_addr, seed_event_graph

def new_nodes(seed_addr, starting_port=STARTING_PORT):
    p2ps = []
    event_graphs = []
    for i in range(1, N):

        inbound_port = starting_port + i
        external_port = starting_port + i
        node_id = str(inbound_port)
        inbound_addrs = [p2p.Url("tcp://127.0.0.1:{}".format(inbound_port))]
        external_addrs = [p2p.Url("tcp://127.0.0.1:{}".format(inbound_port))]
        peers = [p2p.Url("tcp://127.0.0.1:{}".format(starting_port+j)) for j in range(1,N) ]
        seeds = [seed_addr]
        app_version = p2p.new_version(0, 1, 1, '')
        allowed_transports = ['tcp']
        transport_mixing = True
        outbound_connections = N
        inbound_connections = 1000
        outbound_connect_timeout = 15
        channel_handshake_timeout = 15
        channel_heartbeat_interval = 30
        localnet = True
        outbound_peer_discovery_cooloff_time = 30
        outbound_peer_discovery_attempt_time = 5
        p2p_datastore = P2PDATASTORE_PATH+'{}'.format(0)
        hostlist = ''
        greylist_refinery_internval = 15
        white_connect_percnet = 70
        gold_connect_count = 2
        slot_preference_strict = False
        time_with_no_connections = 30
        blacklist = []
        ban_policy = p2p.get_strict_banpolicy()
        settings = p2p.new_settings(
            node_id,
            inbound_addrs,
            external_addrs,
            peers,
            seeds,
            app_version,
            allowed_transports,
            transport_mixing,
            outbound_connections,
            inbound_connections,
            outbound_connect_timeout,
            channel_handshake_timeout,
            channel_heartbeat_interval,
            localnet,
            outbound_peer_discovery_cooloff_time,
            outbound_peer_discovery_attempt_time,
            p2p_datastore,
            hostlist,
            greylist_refinery_internval,
            white_connect_percnet,
            gold_connect_count,
            slot_preference_strict,
            time_with_no_connections,
            blacklist,
            ban_policy
        )
        p2p_ptr = asyncio.run(get_fut_p2p(settings))
        sled_db = sled.SledDb(SLED_DB_PATH+'{}'.format(i))
        event_graph = asyncio.run(get_fut_eg(p2p_ptr, sled_db))
        event_graphs+=[event_graph]
        p2ps+=[p2p_ptr]
    return (p2ps, event_graphs)

async def create_new_event(data, event_graph_ptr):
    return await eg.new_event(data, event_graph_ptr)

async def insert_events(node, event):
    ids = await node.dag_insert(event)
    return ids

async def broadcast_event_onp2p(w8_time, p2p_node, event):
    await p2p.broadcast_p2p(w8_time, p2p_node, event)

async def get_event_by_id(event_graph, event_id):
    return await event_graph.dag_get(event_id)

async def dag_sync(node):
    await node.dag_sync()


############################
#       create seed        #
############################
seed_p2p_ptr, seed_addr, seed_event_graph = get_seed_node()
seed_t = threading.Thread(target=asyncio.run, args=(start_p2p(15, seed_p2p_ptr),))
seed_t.start()
seed_register_t = threading.Thread(target=asyncio.run, args=(register_protocol(seed_p2p_ptr, seed_event_graph),))
seed_register_t.start()
############################
#      create N nodes      #
############################
p2ps, egs = new_nodes(seed_addr)

############################
#     register node        #
############################
register_ts = [seed_register_t]
for idx, node in enumerate(p2ps):
    # register event graph protocol
    eg_t = threading.Thread(target=asyncio.run, args=(register_protocol(node, egs[idx]),))
    eg_t.start()
    register_ts += [eg_t]
for t in register_ts:
    t.join()

###########################
#     start node          #
###########################
start_ts = [seed_t]
for idx, node in enumerate(p2ps):
    # start p2p node
    node_t = threading.Thread(target=asyncio.run, args=(start_p2p(15, node),))
    node_t.start()
    start_ts += [node_t]
#for t in start_ts:
#    t.join()
# wait for peers to connect
time.sleep(10)
greylist_length = asyncio.run(get_greylist_length(seed_p2p_ptr))
print("greylist_len: {}".format(greylist_length))
assert(greylist_length==N-1)

# select random node
rnd_idx = get_random_node_idx()
random_node = egs[rnd_idx]
print('random node of index {} was selected: {}'.format(rnd_idx, egs[rnd_idx]))

for evg in egs:
     assert(evg.dag_len()==1)

# create new event
event = asyncio.run(create_new_event([1,2,3,4], random_node))
print("event: {}".format(event))

# insert event at random node
ids = asyncio.run(insert_events(random_node, [event]))
print("dag id: {}".format(ids[0]))

# broadcast the new event
#random_node_p2p = p2ps[rnd_idx]
# broadcast to seed node isntead
random_node_p2p = seed_p2p_ptr
threading.Thread(target=asyncio.run, args=(broadcast_event_onp2p(15, random_node_p2p, event),)).start()


'''
dag_ts = []

print("=======================")
print("=      dag sync       =")
print("=======================")

# dag sync

for eg in egs:
    dag_t = threading.Thread(target=asyncio.run, args=(dag_sync(eg),))
    dag_t.start()
    dag_ts+=[dag_t]

for t in dag_ts:
    t.join()

# get broadcasted event
event2 = asyncio.run(get_event_by_id(egs[rnd_idx], ids[0]))
print("broadcasted event: {}".format(event2))
'''

# assert event is broadcast to all nodes
# FIXME
for evg in egs:
    print("len: {}".format(evg.dag_len()))
    #assert(evg.dag_len()==(N-1))
