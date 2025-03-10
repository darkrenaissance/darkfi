from darkfi_eventgraph_py import p2p
from consts import *
from utils import start_p2p, stop_p2p, get_fut_p2p, is_connected
import asyncio
import threading
import time

def get_peer_node(i, seed_addr, starting_port=STARTING_PORT):
    inbound_port = starting_port + i
    external_port = starting_port + i
    print("node with port: {}".format(inbound_port))
    addrs = p2p.Url("tcp://127.0.0.1:{}".format(inbound_port))
    inbound_addrs = [addrs]
    external_addrs = [addrs]
    node_id = str(inbound_port)
    peers = []
    seeds = [seed_addr]
    magic_bytes = p2p.MagicBytes(MAGIC_CODE)
    app_version = p2p.new_version(0, 1, 1, '')
    allowed_transports = ['tcp']
    transport_mixing = True
    outbound_connections = 100
    inbound_connections = 10000
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
    ban_policy = p2p.get_relaxed_banpolicy()
    settings = p2p.new_settings(
        node_id,
        inbound_addrs,
        external_addrs,
        peers,
        seeds,
        magic_bytes,
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
    return p2p_ptr

def get_seed_node(starting_port=STARTING_PORT):
    inbound_port = starting_port
    node_id = str(inbound_port)
    print("seed with port: {}".format(inbound_port))
    seed_addr = p2p.Url("tcp://127.0.0.1:{}".format(inbound_port))
    inbound_addrs = [seed_addr]
    external_addrs = [seed_addr]
    peers = []
    seeds = []
    magic_bytes = p2p.MagicBytes(MAGIC_CODE)
    app_version = p2p.new_version(0, 1, 1, '')
    allowed_transports = ['tcp']
    transport_mixing = True
    outbound_connections = 0
    inbound_connections = 10000
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
    ban_policy = p2p.get_relaxed_banpolicy()
    settings = p2p.new_settings(
        node_id,
        inbound_addrs,
        external_addrs,
        peers,
        seeds,
        magic_bytes,
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
    return seed_p2p_ptr, seed_addr

# create p2p node
def new_nodes(seed_addr, starting_port=STARTING_PORT):
    nodes = []
    for i in range(1, N):
        print("=====================================")
        print("    initializing  nodes...            ")
        print("=====================================")
        p2p_ptr = get_peer_node(i, seed_addr)
        # start p2p node
        nodes+=[p2p_ptr]
    return nodes

# create N nodes
seed_p2p_ptr, seed_addr = get_seed_node()
print("=====================================")
print("starting seed node on {}".format(seed_addr))
print("=====================================")

start_ts = []
seed_t = threading.Thread(target=asyncio.run, args=(start_p2p(W8_TIME, seed_p2p_ptr),))
seed_t.start()
start_ts+=[seed_t]
p2ps = new_nodes(seed_addr)
for idx, node in enumerate(p2ps):
    print("========================================")
    print("starting node: {}".format(node))
    print("========================================")
    node_t = threading.Thread(target=asyncio.run, args=(start_p2p(W8_TIME, node),))
    node_t.start()
    start_ts+=[node_t]

# wait for peers to connect
print("wait {} secs for nodes to connect".format(W8_TIME_4_CON))
time.sleep(W8_TIME_4_CON)

for t in start_ts:
    t.join()

for node in p2ps:
    assert(is_connected(node))
    print('node: {} is connected successfully'.format(node))

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
