# darkfi-eventgraph-py

Python bindings for event-graph

## Build and install

1. Install `maturin` via your package manager or from source.
2. Run `make` to build the wheel
3. (Optional) Run pip install --user <path_to_wheel>

## Development

For a development version you can use a venv:

```
$ python3 -m venv venv
$ source venv/bin/activate
(venv) $ make dev
```

## usage

``` python
from darkfi_eventgraph_py import event_graph as eg, p2p, sled
import asyncio

node_id = ''
inbound_addrs = [p2p.Url("tcp://127.0.0.1:53416")]
external_addrs = [p2p.Url("tcp://127.0.0.1:53416")]
peers = [p2p.Url("tcp://127.0.0.1:5345"), p2p.Url("tcp://127.0.0.1:53416")]
seeds = []
app_version = p2p.new_version(0, 1, 1, '')
allowed_transports = []#['tcp+tls']
transport_mixing = False
outbound_connections = 2
inbound_connections = 8
outbound_connect_timeout = 15
channel_handshake_timeout = 10
channel_heartbeat_interval = 30
localnet = True
outbound_peer_discovery_cooloff_time = 30
outbound_peer_discovery_attempt_time = 5
p2p_datastore = '/tmp/p2pdatastore'
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

# create p2p node
async def new_p2p():
    p2p_ptr = await p2p.new_p2p(settings)
    return p2p_ptr
p2p = asyncio.run(new_p2p())

# create sled database
sled_db = sled.SledDb('/tmp/sleddb')

async def new_eg():
    return await eg.new_event_graph(p2p, sled_db, p2p_datastore, True, '', 0)
event_graph = asyncio.run(new_eg())

```

## pyo3 warnings

if you see build warning ["non-local impl definition"](https://github.com/PyO3/pyo3/discussions/4083), ignore it, it's an issue with old pyo3 version, that was patched, in later versions, we forced to use pyo3 version 0.2 due to unmaintained pyo3-asyncio which depend on 0.2 pyo3.
