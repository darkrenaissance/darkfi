from darkfi_eventgraph_py import p2p

async def start_p2p(w8_time, node):
    await p2p.start_p2p(w8_time, node)

async def stop_p2p(w8_time, node):
    await p2p.stop_p2p(w8_time, node)

async def get_fut_p2p(settings):
    node = await p2p.new_p2p(settings)
    return node

def is_connected(node):
    return p2p.is_connected(node)
