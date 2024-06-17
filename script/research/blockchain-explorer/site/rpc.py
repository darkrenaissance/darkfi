import asyncio, json, random
from flask import abort

# TODO have a single channel for the whole app
# Class representing the channel with the JSON-RPC server.
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

# Create a Channel for given server.
async def create_channel(server_name, port):
    try:
        reader, writer = await asyncio.open_connection(server_name, port)
    except ConnectionRefusedError:
        print(f"Error: Connection Refused to '{server_name}:{port}', Either because the daemon is down, is currently syncing or wrong url.")
        abort(500)
    channel = Channel(reader, writer)
    return channel


# Execute a request towards the JSON-RPC server
async def query(method, params, server_name, port):
    channel = await create_channel(server_name, port)
    request = {
        "id": random.randint(0, 2**32),
        "method": method,
        "params": params,
        "jsonrpc": "2.0",
    }
    await channel.send(request)

    response = await channel.receive()
    # Closed connect returns None
    if response is None:
        print("error: connection with server was closed", file=sys.stderr)
        abort(500)

    if "error" in response:
        error = response["error"]
        errcode, errmsg = error["code"], error["message"]
        print(f"error: {errcode} - {errmsg}", file=sys.stderr)
        abort(500)

    return response["result"]

# Retrieve last n blocks from blockchain-explorer daemon
async def get_last_n_blocks(n, server_name, port):
    return await query("blocks.get_last_n_blocks", [str(n)], server_name, int(port))

# Retrieve basic statistics from blockchain-explorer daemon
async def get_basic_statistics(server_name, port):
    return await query("statistics.get_basic_statistics", [], server_name, int(port))
