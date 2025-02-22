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

"""
Module: rpc.py

This module provides an asynchronous interface for interacting with the DarkFi explorer daemon
using JSON-RPC. It includes functionality to create a communication channel, send requests,
and handle responses from the server.
"""

import asyncio, json, random

from flask import abort, current_app

class Channel:
    """Class representing the channel with the JSON-RPC server."""
    def __init__(self, reader, writer):
        """Initialize the channel with a reader and writer."""
        self.reader = reader
        self.writer = writer

    async def readline(self):
        """Read a line from the channel, closing it if the connection is lost."""
        if not (line := await self.reader.readline()):
            self.writer.close()
            return None
        return line[:-1].decode()  # Strip the newline

    async def receive(self):
        """Receive and decode a message from the channel."""
        if (plaintext := await self.readline()) is None:
            return None

        message = plaintext
        response = json.loads(message)
        return response

    async def send(self, obj):
        """Send a JSON-encoded object to the channel."""
        message = json.dumps(obj)
        data = message.encode()

        self.writer.write(data + b"\n")
        await self.writer.drain()

async def create_channel(server_name, port):
    """
     Creates a channel used to send RPC requests to the DarkFi explorer daemon.
    """
    try:
        reader, writer = await asyncio.open_connection(server_name, port)
    except ConnectionRefusedError:
        print(
            f"Error: Connection Refused to '{server_name}:{port}', Either because the daemon is down, is currently syncing or wrong url.")
        abort(500)
    channel = Channel(reader, writer)
    return channel

async def query(method, params):
    """
     Execute a request towards the JSON-RPC server by constructing a JSON-RPC
     request and sending it to the server. It handles connection errors and server responses,
     returning the result of the query or raising an error if the request fails.
    """
    # Create the channel to send RPC request
    channel = await create_channel(current_app.config['EXPLORER_RPC_URL'], current_app.config['EXPLORER_RPC_PORT'])

    # Prepare request
    request = {
        "id": random.randint(0, 2 ** 32),
        "method": method,
        "params": params,
        "jsonrpc": "2.0",
    }

    # Send request and await response
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

async def get_last_n_blocks(n: str):
    """Retrieves the last n blocks."""
    return await query("blocks.get_last_n_blocks", [n])

async def get_basic_statistics():
    """Retrieves basic statistics."""
    return await query("statistics.get_basic_statistics", [])

async def get_metric_statistics():
    """Retrieves metrics statistics."""
    return await query("statistics.get_metric_statistics", [])

async def get_block(header_hash: str):
    """Retrieves block information for a given header hash."""
    return await query("blocks.get_block_by_hash", [header_hash])

async def get_block_transactions(header_hash: str):
    """Retrieves transactions associated with a given block header hash."""
    return await query("transactions.get_transactions_by_header_hash", [header_hash])


async def get_transaction(transaction_hash: str):
    """Retrieves transaction information for a given transaction hash."""
    return await query("transactions.get_transaction_by_hash", [transaction_hash])

async def get_native_contracts():
    """Retrieves native contracts."""
    return await query("contracts.get_native_contracts", [])


async def get_contract_source_paths(contract_id: str):
    """Retrieves contract source code paths for a given contract ID."""
    return await query("contracts.get_contract_source_code_paths", [contract_id])

async def get_contract_source(contract_id: str, source_path):
    """Retrieves the contract source file for a given contract ID and source path."""
    return await query("contracts.get_contract_source", [contract_id, source_path])
