import json

class Channel:

    def __init__(self, reader, writer):
        self.reader = reader
        self.writer = writer

    async def readline(self):
        if not (line := await self.reader.readline()):
            self.writer.close()
            return None
        # Strip the newline
        return line[:-1].decode()

    async def receive(self):
        if (plaintext := await self.readline()) is None:
            return None

        message = plaintext
        response = json.loads(message)
        return response

    async def send(self, obj):
        message = json.dumps(obj)
        data = message.encode()

        self.writer.write(data + b"\n")
        await self.writer.drain()

async def _test_client():
    await asyncio.sleep(1)
    reader, writer = await asyncio.open_connection("127.0.0.1", 7643)
    channel = Channel(reader, writer)
    request = {
        "foo": "bar"
    }
    await channel.send(request)
    response = await channel.receive()
    print(f"Client: {response}")

async def _test_server(reader, writer):
    channel = Channel(reader, writer)
    request = await channel.receive()
    print(f"Server: {request}")
    response = {
        "abc": "xyz"
    }
    await channel.send(response)

async def _test_channel():
    server = await asyncio.start_server(_test_server, "127.0.0.1", 7643)
    task1 = asyncio.create_task(_test_client())
    async with server:
        task2 = asyncio.create_task(server.serve_forever())
        await asyncio.sleep(3)
    await task1
    task2.cancel()

if __name__ == "__main__":
    import asyncio
    # run send and recv testes
    asyncio.run(_test_channel())

