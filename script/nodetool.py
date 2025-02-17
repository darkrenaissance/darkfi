#!/usr/bin/env python

# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2025 Dyne.org foundation
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as
# published by the Free Software Foundation, either version 3 of the
# License, or (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.

import asyncio, json, random, sys, time

class JsonRpc:

    async def start(self, server, port):
        reader, writer = await asyncio.open_connection(server, port)
        self.reader = reader
        self.writer = writer

    async def stop(self):
        self.writer.close()
        await self.writer.wait_closed()

    async def _make_request(self, method, params):
        ident = random.randint(0, 2**16)
        print(ident)
        request = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": ident,
        }

        message = json.dumps(request) + "\n"
        self.writer.write(message.encode())
        await self.writer.drain()

        data = await self.reader.readline()
        message = data.decode().strip()
        response = json.loads(message)
        print(response)
        return response

    async def _subscribe(self, method, params):
        ident = random.randint(0, 2**16)
        request = {
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": ident,
        }

        message = json.dumps(request) + "\n"
        self.writer.write(message.encode())
        await self.writer.drain()
        print("Subscribed")

    async def ping(self):
        return await self._make_request("ping", [])

    async def dnet_switch(self, state):
        return await self._make_request("dnet.switch", [state])

    async def dnet_subscribe_events(self):
        return await self._subscribe("dnet.subscribe_events", [])

async def main(argv):
    rpc = JsonRpc()
    while True:
        try:
            await rpc.start("localhost", 26660)
            break
        except OSError:
            pass
    await rpc.dnet_switch(True)
    await rpc.dnet_subscribe_events()

    while True:
        data = await rpc.reader.readline()
        #with open("rpclog", "a") as f:
        #    f.write(data.decode())
        data = json.loads(data)

        params = data["params"][0]
        ev = params["event"]
        if ev in ["send", "recv"]:
            continue
        info = params["info"]

        t = time.localtime()
        current_time = time.strftime("%H:%M:%S", t)

        match ev:
            case "inbound_connected":
                addr = info["addr"]
                print(f"{current_time}  inbound (connect):    {addr}")
            case "inbound_disconnected":
                addr = info["addr"]
                print(f"{current_time}  inbound (disconnect): {addr}")
            case "outbound_slot_sleeping":
                slot = info["slot"]
                print(f"{current_time}  slot {slot}: sleeping")
            case "outbound_slot_connecting":
                slot = info["slot"]
                addr = info["addr"]
                print(f"{current_time}  slot {slot}: connecting   addr={addr}")
            case "outbound_slot_connected":
                slot = info["slot"]
                addr = info["addr"]
                channel_id = info["channel_id"]
                print(f"{current_time}  slot {slot}: connected    addr={addr}")
            case "outbound_slot_disconnected":
                slot = info["slot"]
                err = info["err"]
                print(f"{current_time}  slot {slot}: disconnected err='{err}'")
            case "outbound_peer_discovery":
                attempt = info["attempt"]
                state = info["state"]
                print(f"{current_time}  peer_discovery: {state} (attempt {attempt})")
        #print(data)

    await rpc.dnet_switch(False)
    await rpc.stop()

asyncio.run(main(sys.argv))
