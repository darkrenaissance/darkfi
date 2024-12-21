from darkfi_eventgraph_py import p2p, sled
import asyncio
import random
import time
import subprocess
import os
import threading
# number of nodes
N = 4
P2PDATASTORE_PATH = '/tmp/p2pdatastore'
STARTING_PORT = 53412
os.system("rm -rf " + P2PDATASTORE_PATH+"*")

async def start_p2p(node):
    await p2p.start_p2p(node)

async def stop_p2p(node):
    await p2p.stop_p2p(node)

async def get_fut_p2p(settings):
    node = await p2p.new_p2p(settings)
    #print("new p2p node created: {}".format(node))
    return node

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
    inbound_connections = 10000
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
    return seed_p2p_ptr, seed_addr

def get_peer_node(i, seed_addr, starting_port=STARTING_PORT):
    inbound_port = starting_port + i
    external_port = starting_port + i
    inbound_addrs = [p2p.Url("tcp://127.0.0.1:{}".format(inbound_port))]
    external_addrs = [p2p.Url("tcp://127.0.0.1:{}".format(inbound_port))]
    node_id = str(inbound_port)
    peers = [p2p.Url("tcp://127.0.0.1:{}".format(starting_port+j)) for j in range(1,N) ]
    seeds = [seed_addr]
    app_version = p2p.new_version(0, 1, 1, '')
    allowed_transports = ['tcp']
    transport_mixing = True
    outbound_connections = N
    inbound_connections = 10000
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
    return p2p_ptr

# create p2p node
def new_nodes(seed_addr, starting_port=STARTING_PORT):
    nodes = []
    for i in range(1, N):
        print("=====================================")
        print("    initializing  nodes...           ")
        print("=====================================")
        p2p_ptr = get_peer_node(i, seed_addr)
        # start p2p node
        nodes+=[p2p_ptr]
    return nodes

async def get_greylist_length(node):
    return await p2p.get_greylist_length(node)


async def get_whitelist_length(node):
    return await p2p.get_whitelist_length(node)


async def get_goldlist_length(node):
    return await p2p.get_goldlist_length(node)

# create N nodes
seed_p2p_ptr, seed_addr = get_seed_node()
print("=====================================")
print("starting seed node on {}".format(seed_addr))
print("=====================================")

ts = []
start_p2p_coroutine = start_p2p(seed_p2p_ptr)
seed_t = threading.Thread(target=asyncio.run, args=(start_p2p_coroutine,))
seed_t.start()
ts += [seed_t]
p2ps = new_nodes(seed_addr)
for node in p2ps:
    print("========================================")
    print("starting node: {}".format(node))
    print("========================================")
    node_t = threading.Thread(target=asyncio.run, args=(start_p2p(node),))
    node_t.start()
    ts += [node_t]
for t in ts:
    t.join()

greylist_length = asyncio.run(get_greylist_length(seed_p2p_ptr))
assert(greylist_length==N-1)

print("========================================")
print("=        stop nodes                    =")
print("========================================")
stop_ts = []
seed_t = threading.Thread(target=asyncio.run, args=(stop_p2p(seed_p2p_ptr),))
seed_t.start()
stop_ts += [seed_t]
# stop the nodes first, then the seed.
for node in p2ps:
    node_t = threading.Thread(target=asyncio.run, args=(stop_p2p(node),))
    node_t.start()
    stop_ts += [node_t]
for t in stop_ts:
    t.join()
