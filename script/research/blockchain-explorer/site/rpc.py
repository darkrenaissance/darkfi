# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2024 Dyne.org foundation
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

import asyncio, json, random
from flask import abort

# DarkFi blockchain-explorer daemon JSON-RPC configuration
URL = "127.0.0.1"
PORT = 14567

# Class representing the channel with the JSON-RPC server
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
async def query(method, params):
    channel = await create_channel(URL, PORT)
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
        print("error: connection with server was closed")
        abort(500)

    # Erroneous query is handled with not found
    if "error" in response:
        error = response["error"]
        errcode, errmsg = error["code"], error["message"]
        print(f"error: {errcode} - {errmsg}")
        abort(404)

    return response["result"]

# Retrieve last n blocks from blockchain-explorer daemon
async def get_last_n_blocks(n: str):
    return await query("blocks.get_last_n_blocks", [n])

# Retrieve basic statistics from blockchain-explorer daemon
async def get_basic_statistics():
    return await query("statistics.get_basic_statistics", [])

# Retrieve fee data statistics from blockchain-explorer daemon
async def get_metric_statistics():
    return await query("statistics.get_metric_statistics", [])

# Retrieve the block information of given header hash from blockchain-explorer daemon
async def get_block(header_hash: str):
    return await query("blocks.get_block_by_hash", [header_hash])

# Retrieve the transactions of given block header hash from blockchain-explorer daemon
async def get_block_transactions(header_hash: str):
    return await query("transactions.get_transactions_by_header_hash", [header_hash])

# Retrieve the transaction information of given hash from blockchain-explorer daemon
async def get_transaction(transaction_hash: str):
    return await query("transactions.get_transaction_by_hash", [transaction_hash])
