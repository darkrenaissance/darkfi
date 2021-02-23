import asyncio
from tabulate import tabulate
from copy import deepcopy
import re
import os
import sys
import time

lock = asyncio.Lock()
logs_path = "/tmp/darkfi/"

node_info = {
}

ping_times = {
}

def debug(line):
    #print(line)
    pass

def process(info, line):
    regex_inbound_connect = re.compile(
        ".* Connected inbound \[(\d+[.]\d+[.]\d+[.]\d+:\d+)\]")
    regex_outbound_slots = re.compile(
        ".* Starting (\d+) outbound connection slots.")
    regex_outbound_connect = re.compile(
        ".* #(\d+) connected to outbound \[(\d+[.]\d+[.]\d+[.]\d+:\d+)\]")
    regex_channel_disconnected = re.compile(
        ".* Channel (\d+[.]\d+[.]\d+[.]\d+:\d+) disconnected")
    regex_pong_recv = re.compile(
        ".* Received Pong message (\d+)ms from \[(\d+[.]\d+[.]\d+[.]\d+:\d+)\]")

    if "net: P2p::start() [BEGIN]" in line:
        info["status"] = "p2p-start"
    elif "net: SeedSession::start() [START]" in line:
        info["status"] = "seed-start"
    elif "net: SeedSession::start() [END]" in line:
        info["status"] = "seed-done"
    elif "net: P2p::start() [END]" in line:
        info["status"] = "p2p-done"
    elif "net: P2p::run() [BEGIN]" in line:
        info["status"] = "p2p-run"
    elif "Not configured for accepting incoming connections." in line:
        info["inbounds"] = ["Disabled"]
    elif (match := regex_inbound_connect.match(line)) is not None:
        address = match.group(1)
        info["inbounds"].append(address)
    elif (match := regex_outbound_slots.match(line)) is not None:
        slots = match.group(1)
        info["outbounds"] = ["None" for _ in range(int(slots))]
    elif (match := regex_outbound_connect.match(line)) is not None:
        slot = match.group(1)
        address = match.group(2)
        info["outbounds"][int(slot)] = address
    elif (match := regex_channel_disconnected.match(line)) is not None:
        address = match.group(1)
        try:
            info["inbounds"].remove(address)
        except ValueError:
            pass
        try:
            idx = info["outbounds"].index(address)
            info["outbounds"][idx] = "None"
        except ValueError:
            pass
    elif (match := regex_pong_recv.match(line)) is not None:
        ping_time = match.group(1)
        address = match.group(2)
        ping_times[address] = ping_time

async def scanner(filename):
    global table_data

    async with lock:
        node_info[filename] = {
            "status": "none",
            "inbounds": [],
            "outbounds": [],
        }
        info = node_info[filename]

    with open(logs_path + filename) as fileh:
        while True:
            line = fileh.readline()
            if line:
                debug("R: " + filename + ": " + line[:-1])
                async with lock:
                    process(info, line)
            else:
                await asyncio.sleep(0.5)

def clear_lines(n):
    for i in range(n):
        sys.stdout.write('\033[F')

def get_ping(addr):
    ping_time = "none"
    if addr in ping_times:
        ping_time = str(ping_times[addr]) + " ms"
    return ping_time

def table_format(ninfo):
    table_data = []
    for filename, info in ninfo.items():
        table_data.append([filename, "", ""])
        table_data.append(["", "status", info["status"]])

        inbounds = info["inbounds"]
        if inbounds:
            table_data.append(["", "inbounds", inbounds[0],
                               get_ping(inbounds[0])])

            for inbound in inbounds[1:]:
                table_data.append(["", "", inbound, get_ping(inbound)])

        outbounds = info["outbounds"]
        if outbounds:
            table_data.append(["", "outbounds", outbounds[0],
                               get_ping(outbounds[0])])

            for outbound in outbounds[1:]:
                table_data.append(["", "", outbound, get_ping(outbound)])

    headers = ["Name", "Attribute", "Value", "Ping Times"]
    return headers, table_data

async def refresh_table(tick=1):
    for filename in os.listdir(logs_path):
        asyncio.create_task(scanner(filename))

    previous_lines = 0

    while True:
        clear_lines(previous_lines)

        async with lock:
            ninfo = deepcopy(node_info)
        headers, table_data = table_format(ninfo)
        lines = tabulate(table_data, headers=headers).split("\n")
        debug("-------------------")
        for line in lines:
            print('\x1b[2K\r', end="")
            print(line)

        previous_lines = len(lines)

        await asyncio.sleep(1)

asyncio.run(refresh_table())

