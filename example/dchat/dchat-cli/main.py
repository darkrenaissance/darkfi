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
        try:
            reader, writer = await asyncio.open_connection(server, port)
            self.reader = reader
            self.writer = writer
        except ConnectionRefusedError:
            print(f"Error: Connection Refused to '{server}:{port}'")
            sys.exit(-1)

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

    async def send(self, message):
        return await self._make_request("send", [message])

    async def recv(self):
        return await self._make_request("recv", [])


async def main(argv):
    endpoint = "localhost:51054"
    for i in range(1, len(argv)):
        if argv[i] == "-e":
            endpoint = argv[i+1]
            del argv[i]
            del argv[i]
            break

    if len(argv) == 1 or any(x in ["h", "-h", "--help", "help"] for x in argv):
        print('''USAGE
    python main.py [OPTIONS] [SUBCOMMAND]

    OPTIONS:
        -h, --help      Print help information
        -e              RPC endpoint [default: localhost:51345]

    SUBCOMMANDS:
        send        Send a message
        recv        Receive messages
        ping        Send ping
        help        show this help text

    Examples:
        python main.py -e localhost:52345 send "Hello"
        python main.py send "Hello World!"
        python main.py recv
        python main.py ping
    ''')

        return 0
    
    server, port = endpoint.split(":")
    rpc = JsonRpc()
    await rpc.start(server, port)

    if argv[1] == "send":
        if len(argv) > 2:
            await rpc.send(argv[2])
        else:
            print("Error: send subcommand needs a message")

    elif argv[1] == "recv":
        await rpc.recv()
    elif argv[1] == "ping":
        await rpc.ping()
    else:
        print(f"Error: Unknown subcommand : {argv[1]}")

    await rpc.stop()


asyncio.run(main(sys.argv))

