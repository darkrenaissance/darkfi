#!/usr/bin/python
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

    async def spawns(self):
        return await self._make_request("spawns", [])

async def main(argv):
    rpc = JsonRpc()
    while True:
        try:
            await rpc.start("localhost", 18927)
            break
        except OSError:
            pass
    response = await rpc._make_request("spawns", [])
    info = response["result"]
    spawns = info["spawns"]

    for spawn in spawns:
        urls = spawn["urls"]
        name = spawn["name"]
        whitelist = spawn["whitelist"]
        greylist = spawn["greylist"]
        anchorlist = spawn["anchorlist"]

        print(f"\nname: {name}")
        print(f"urls:")
        for url in urls:
            print(f"    {url}")
        if whitelist:
            print(f"whitelist:")
            for host in whitelist:
                print(f"    {host}")
        if greylist:
            print(f"greylist:")
            for host in greylist:
                print(f"    {host}")
        if anchorlist:
            print(f"anchorlist:")
            for host in anchorlist:
                print(f"    {host}")
    await rpc.stop()

asyncio.run(main(sys.argv))
