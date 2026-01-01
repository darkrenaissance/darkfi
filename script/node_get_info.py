#!/usr/bin/env python

# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2026 Dyne.org foundation
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
        #print(ident)
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
        #print(response)
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
        #print("Subscribed")

    async def ping(self):
        return await self._make_request("ping", [])

    async def dnet_switch(self, state):
        return await self._make_request("dnet.switch", [state])

    async def dnet_subscribe_events(self):
        return await self._subscribe("dnet.subscribe_events", [])

    async def get_info(self):
        return await self._make_request("p2p.get_info", [])

async def main(argv):
    rpc = JsonRpc()
    while True:
        try:
            await rpc.start("localhost", 26660)
            break
        except OSError:
            pass
    response = await rpc._make_request("p2p.get_info", [])
    if "error" in response:
        print("Error: ", response["error"])
        await rpc.stop()
        return
    info = response["result"]
    channels = info["channels"]
    channel_lookup = {}
    for channel in channels:
        id = channel["id"]
        channel_lookup[id] = channel

    print("inbound:")
    for channel in channels:
        if channel["session"] != "inbound":
            continue
        url = channel["url"]
        print(f"  {url}")

    print("outbound:")
    for i, id in enumerate(info["outbound_slots"]):
        if id == 0:
            print(f"  {i}: none")
            continue

        assert id in channel_lookup
        url = channel_lookup[id]["url"]
        print(f"  {i}: {url}")

    print("seed:")
    for channel in channels:
        if channel["session"] != "seed":
            continue
        url = channel["url"]
        print(f"  {url}")

    print("manual:")
    for channel in channels:
        if channel["session"] != "manual":
            continue
        url = channel["url"]
        print(f"  {url}")

    await rpc.stop()

if __name__ == "__main__":
    asyncio.run(main(sys.argv))

