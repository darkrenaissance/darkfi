import asyncio, json, random
import sys

# import lib.config
from lib.net import Channel

async def create_channel(server_name, port):
    try:
        reader, writer = await asyncio.open_connection(server_name, port)
    except ConnectionRefusedError:
        print(f"Error: Connection Refused to '{server_name}:{port}', Either because the daemon is down, is currently syncing or wrong url.")
        sys.exit(-1)
    channel = Channel(reader, writer)
    return channel

def random_id():
    return random.randint(0, 2**32)

async def query(method, params, server_name, port):
    channel = await create_channel(server_name, port)
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

async def get_info(server_name, port):
    return await query("get_info", [], server_name, int(port))

async def get_workspace(server_name, port):
    return await query("get_ws", [], server_name, int(port))

async def add_task(task, server_name, port):
    return await query("add", [task], server_name, int(port))

async def get_ref_ids(server_name, port):
    return await query("get_ref_ids", [], server_name, int(port))

async def get_archive_ref_ids(month_ts, server_name, port):
    return await query("get_archive_ref_ids", [str(month_ts)], server_name, int(port))

async def fetch_task(refid, server_name, port):
    return await query("get_task_by_ref_id", [refid], server_name, int(port))

async def change_task_status(refid, status, server_name, port):
    await query("set_state", [refid, status], server_name, int(port))
    return True

async def modify_task(refid, changes, server_name, port):
    return await query("modify", [refid, changes], server_name, int(port))

async def switch_workspace(workspace, server_name, port):
    return await query("switch_ws", [workspace], server_name, int(port))

async def fetch_active_tasks(server_name, port):
    return await query("fetch_active_tasks", [], server_name, int(port))

async def fetch_deactive_tasks(month_ts, server_name, port):
    return await query("fetch_deactive_tasks", [str(month_ts)], server_name, int(port))

async def fetch_archive_task(task_refid, month_ts, server_name, port):
    return await query("fetch_archive_task", [task_refid, str(month_ts)], server_name, int(port))

async def add_task_comment(refid, comment, server_name, port):
    return await query("set_comment", [refid, comment], server_name, int(port))

async def export_to(path, server_name, port):
    return await query("export", [path], server_name, int(port))

async def import_from(path, server_name, port):
    return await query("import", [path], server_name, int(port))
