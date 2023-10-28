import json, os, sys
from Crypto.Cipher import AES

# import lib.config

# should be 32 bytes hex
# https://pycryptodome.readthedocs.io/en/latest/src/cipher/aes.html
# KEY = bytes.fromhex(lib.config.get(
#     "shared_secret",
#     "87b9b70e722d20c046c8dba8d0add1f16307fec33debffec9d001fd20dbca3ee"
# ))

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
        # if (ciphertext := await self.readline()) is None:
        #     return None
        # if (tag := await self.readline()) is None:
        #     return None

        # nonce = bytes.fromhex(nonce)
        # ciphertext = bytes.fromhex(ciphertext)
        # tag = bytes.fromhex(tag)

        #print(f"{nonce.hex()}")
        #print(f"{ciphertext.hex()}")
        #print(f"{tag.hex()}")
        #print()

        # Decrypt
        # cipher = AES.new(KEY, AES.MODE_EAX, nonce=nonce)
        # plaintext = cipher.decrypt(ciphertext)
        # try:
        #     cipher.verify(tag)
        # except ValueError:
        #     print("error: key incorrect or message corrupted", file=sys.stderr)
        #     return None

        message = plaintext
        response = json.loads(message)
        return response

    async def send(self, obj):
        message = json.dumps(obj)
        data = message.encode()

        # Encrypt
        # cipher = AES.new(KEY, AES.MODE_EAX)
        # nonce = cipher.nonce
        # ciphertext, tag = cipher.encrypt_and_digest(data)

        #print(f"{nonce.hex()}")
        #print(f"{ciphertext.hex()}")
        #print(f"{tag.hex()}")
        #print()

        # Encode as hex strings since the bytes might contain new lines
        # nonce = nonce.hex().encode()
        # ciphertext = ciphertext.hex().encode()
        # tag = tag.hex().encode()

        # self.writer.write(nonce + b"\n")
        # self.writer.write(ciphertext + b"\n")
        # self.writer.write(tag + b"\n")
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

