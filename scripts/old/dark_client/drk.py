#!/usr/bin/env python

from util import arg_parser

import aiohttp
import asyncio

class DarkClient:
    # TODO: generate random ID (4 byte unsigned int) (rand range 0 - max size
    # uint32
    def __init__(self, client_session):
        self.url = "http://localhost:8000/"
        self.client_session = client_session
        self.payload = {
                "method": [],
                "params": [],
                "jsonrpc": [],
                "id": [],
                }

    async def key_gen(self, payload):
        payload['method'] = "key_gen"
        payload['jsonrpc'] = "2.0"
        payload['id'] = "0"
        key = await self.__request(payload)
        print(key)

    async def get_info(self, payload):
        payload['method'] = "get_info"
        payload['jsonrpc'] = "2.0"
        payload['id'] = "0"
        info = await self.__request(payload)
        print(info)

    async def stop(self, payload):
        payload['method'] = "stop"
        payload['jsonrpc'] = "2.0"
        payload['id'] = "0"
        stop = await self.__request(payload)
        print(stop)

    async def say_hello(self, payload):
        payload['method'] = "say_hello"
        payload['jsonrpc'] = "2.0"
        payload['id'] = "0"
        hello = await self.__request(payload)
        print(hello)

    async def create_wallet(self, payload):
        payload['method'] = "create_wallet"
        payload['jsonrpc'] = "2.0"
        payload['id'] = "0"
        wallet = await self.__request(payload)
        print(wallet)

    async def create_cashier_wallet(self, payload):
        payload['method'] = "create_cashier_wallet"
        payload['jsonrpc'] = "2.0"
        payload['id'] = "0"
        wallet = await self.__request(payload)
        print(wallet)

    async def test_wallet(self, payload):
        payload['method'] = "test_wallet"
        payload['jsonrpc'] = "2.0"
        payload['id'] = "0"
        test = await self.__request(payload)
        print(test)

    async def __request(self, payload):
        async with self.client_session.post(self.url, json=payload) as response:
            resp = await response.text()
            print(resp)

async def main():
    try:
        async with aiohttp.ClientSession() as session:
            client = DarkClient(session)
            await arg_parser(client)
    except aiohttp.ClientConnectorError as err:
        print('CONNECTION ERROR:', str(err))
    except Exception as err:
        print("ERROR: ", str(err))

if __name__ == "__main__":
    loop = asyncio.get_event_loop()
    loop.run_until_complete(main())


