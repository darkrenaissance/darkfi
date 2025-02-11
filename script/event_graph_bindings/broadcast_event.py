from darkfi_eventgraph_py import event_graph as eg, p2p, sled
from consts import *
from utils import start_p2p, stop_p2p, get_fut_p2p, is_connected
import asyncio
import random
import time
import threading

def get_random_node_idx():
    return int(random.random()*(N-1))

def get_new_eg(node, sled_db):
    return eg.new_event_graph(node, sled_db, P2PDATASTORE_PATH, False, 'dag', 1)

async def register_protocol(p2p_node, eg_node):
    await p2p.register_protocol_p2p(p2p_node, eg_node)

# create p2p node
def get_seed_node(starting_port=STARTING_PORT):
    inbound_port = starting_port
    node_id = str(inbound_port)
    seed_addr = p2p.Url("tcp://127.0.0.1:{}".format(inbound_port))
    inbound_addrs = [seed_addr]
    external_addrs = [seed_addr]
    peers = []
    seeds = []
    app_version = p2p.new_version(0, 1, 1, '')
    allowed_transports = ['tcp']
    transport_mixing = True
    outbound_connections = 0
    inbound_connections = 1000
    outbound_connect_timeout = OUTBOUND_TIMEOUT
    channel_handshake_timeout = CH_HANDSHAKE_TIMEOUT
    channel_heartbeat_interval = CH_HEARTBEAT_INTERVAL
    localnet = True
    outbound_peer_discovery_cooloff_time = DISCOVERY_COOLOFF
    outbound_peer_discovery_attempt_time = DISCOVERY_ATTEMPT
    p2p_datastore = P2PDATASTORE_PATH+'{}'.format(0)
    hostlist = ''
    greylist_refinery_internval = REFINERY_INTERVAL
    white_connect_percnet = WHITE_CONNECT_PERCENT
    gold_connect_count = GOLD_CONNECT_COUNT
    slot_preference_strict = False
    time_with_no_connections = TIME_NO_CON
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
    seed_event_graph = get_new_eg(seed_p2p_ptr, seed_sled_db)
    return seed_p2p_ptr, seed_addr, seed_event_graph

def new_nodes(seed_addr, starting_port=STARTING_PORT):
    p2ps = []
    event_graphs = []
    for i in range(1, N):
        inbound_port = starting_port + i
        external_port = starting_port + i
        node_id = str(inbound_port)
        addrs = p2p.Url("tcp://127.0.0.1:{}".format(inbound_port))
        inbound_addrs = [addrs]
        external_addrs = [addrs]
        peers = []
        seeds = [seed_addr]
        app_version = p2p.new_version(0, 1, 1, '')
        allowed_transports = ['tcp']
        transport_mixing = True
        outbound_connections = N
        inbound_connections = 1000
        outbound_connect_timeout = OUTBOUND_TIMEOUT
        channel_handshake_timeout = CH_HANDSHAKE_TIMEOUT
        channel_heartbeat_interval = CH_HEARTBEAT_INTERVAL
        localnet = True
        outbound_peer_discovery_cooloff_time = DISCOVERY_COOLOFF
        outbound_peer_discovery_attempt_time = DISCOVERY_ATTEMPT
        p2p_datastore = P2PDATASTORE_PATH+'{}'.format(0)
        hostlist = ''
        greylist_refinery_internval = REFINERY_INTERVAL
        white_connect_percnet = WHITE_CONNECT_PERCENT
        gold_connect_count = GOLD_CONNECT_COUNT
        slot_preference_strict = False
        time_with_no_connections = TIME_NO_CON
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
        event_graph = get_new_eg(p2p_ptr, sled_db)
        event_graphs+=[event_graph]
        p2ps+=[p2p_ptr]
    return (p2ps, event_graphs)

async def create_new_event(data, event_graph_ptr):
    return await eg.new_event(data, event_graph_ptr)

def insert_events(node, event):
    ids = node.dag_insert(event)
    return ids

async def broadcast_event_onp2p(w8_time, p2p_node, event):
    await p2p.broadcast_p2p(w8_time, p2p_node, event)

async def get_event_by_id(event_graph, event_id):
    return await event_graph.dag_get(event_id)

async def dag_sync(node):
    await node.dag_sync()

def event_id(event):
    return event.id()

############################
#       create seed        #
############################
seed_p2p_ptr, seed_addr, seed_event_graph = get_seed_node()
start_ts = []
seed_t = threading.Thread(target=asyncio.run, args=(start_p2p(W8_TIME, seed_p2p_ptr),))
seed_t.start()
start_ts += [seed_t]
seed_register_t = threading.Thread(target=asyncio.run, args=(register_protocol(seed_p2p_ptr, seed_event_graph),))
seed_register_t.start()

############################
#      create N nodes      #
############################
p2ps, egs = new_nodes(seed_addr)

# select random node
rnd_idx = get_random_node_idx()
random_node = egs[rnd_idx]

for evg in egs:
     assert(evg.dag_len()==1)
# create new event
event = asyncio.run(create_new_event([1,2,3,4], random_node))

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

for node in p2ps:
    # start p2p node
    node_t = threading.Thread(target=asyncio.run, args=(start_p2p(W8_TIME, node),))
    print("starting node {}".format(node))
    node_t.start()
    start_ts += [node_t]

print("wait {} secs for nodes to connect".format(W8_TIME_4_CON))
time.sleep(W8_TIME_4_CON)

# insert event at random node
ids = insert_events(random_node, [event])
print('inserted event ids: {}'.format(str(ids[0])))
# wait for nodes to conenct

# broadcast the new event
random_node_p2p = p2ps[rnd_idx]
print('broadcasting event: {} through node: {}'.format(event, random_node_p2p))
asyncio.run(broadcast_event_onp2p(15, random_node_p2p, event))

for t in start_ts:
    t.join()

for node in p2ps:
    assert(is_connected(node))
    print('node: {} is connected successfully'.format(node))

# get broadcasted event
received_event = asyncio.run(get_event_by_id(egs[rnd_idx], ids[0]))
print("broadcasted event: {}".format(received_event))
broadcasted_event_id = str(event_id(event))
received_event_id = str(event_id(received_event))
assert broadcasted_event_id == received_event_id, '{}, {}'.format(broadcasted_event_id, received_event_id)
time.sleep(5)
# assert event is broadcast to all nodes
for evg in  egs:
    # events + 1 = 2
    assert(evg.dag_len()==2)
    event = asyncio.run(get_event_by_id(evg, ids[0]))

print("Success! joining threads.")
print("========================================")
print("=        stop nodes                    =")
print("========================================")

stop_ts = []
seed_t = threading.Thread(target=asyncio.run, args=(stop_p2p(1, seed_p2p_ptr),))
seed_t.start()
stop_ts += [seed_t]

# stop the nodes first, then the seed.
for node in p2ps:
    node_t = threading.Thread(target=asyncio.run, args=(stop_p2p(2, node),))
    node_t.start()
    stop_ts+=[node_t]

for t in stop_ts:
    t.join()
