import asyncio, json, random, sys

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
            "id": ident
        }

        message = json.dumps(request) + "\n"
        self.writer.write(message.encode())
        await self.writer.drain()

        data = await self.reader.readline()
        message = data.decode()
        response = json.loads(message)
        print(response)
        return "hello"
        #return response["result"]

    async def ping(self):
        return await self._make_request("ping", [])

    async def dnet_switch(self, state):
        return await self._make_request("dnet.switch", [state])

    async def dnet_subscribe_events(self):
        return await self._make_request("dnet.subscribe_events", [])

    #async def dnet_info(self):
    #    return await self._make_request("dnet_info", [])

async def main(argv):
    rpc = JsonRpc()
    await rpc.start("localhost", 26660)
    await rpc.dnet_switch(True)
    await rpc.dnet_subscribe_events()

    while True:
        data = await rpc.reader.readline()
        #print(await rpc.dnet_info())

    await rpc.dnet_switch(False)
    await rpc.stop()

asyncio.run(main(sys.argv))

