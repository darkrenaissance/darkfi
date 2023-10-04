import asyncio, json, random
import sys

# import lib.config
from lib.net import Channel

async def create_channel():
    # server_name = lib.config.get("server", "localhost")
    server_name = "localhost"
    reader, writer = await asyncio.open_connection(server_name, 23341)
    channel = Channel(reader, writer)
    return channel

def random_id():
    return random.randint(0, 2**32)

async def query(method, params):
    channel = await create_channel()
    request = {
        "id": random_id(),
        "method": method,
        "params": params,
        "jsonrpc": "2.0",
    }
    await channel.send(request)

    response = await channel.receive()
    # Closed connect returns None
    if response is None:
        print("error: connection with server was closed", file=sys.stderr)
        sys.exit(-1)

    if "error" in response:
        error = response["error"]
        errcode, errmsg = error["code"], error["message"]
        print(f"error: {errcode} - {errmsg}", file=sys.stderr)
        sys.exit(-1)

    return response["result"]

async def get_info():
    return await query("get_info", [])

async def add_task(task):
    return await query("add", [task])

async def get_ref_ids():
    return await query("get_ref_ids", [])

async def fetch_task(refid):
    return await query("get_task_by_ref_id", [refid])

async def change_task_status(refid, status):
    await query("set_state", [refid, status])
    return True

async def modify_task(refid, changes):
    return await query("modify", [refid, changes])

async def fetch_active_tasks():
    return await query("fetch_active_tasks", [])

async def fetch_deactive_tasks(month):
    return await query("fetch_deactive_tasks", [month])

async def fetch_archive_task(task_refid, month):
    return await query("fetch_archive_task", [task_refid, month])

async def add_task_comment(refid, comment):
    await query("set_comment", [refid, comment])
    return True
