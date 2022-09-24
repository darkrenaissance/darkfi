#!/usr/bin/env python3
import asyncio
import logging

from base58 import b58decode
from nacl.public import PrivateKey, Box

from meetbot_cfg import config

# Initialized channels from the configuration
CHANS = {}


async def channel_listen(host, port, nick, chan):
    global CHANS

    logging.info(f"Connecting to {host}:{port}")
    reader, writer = await asyncio.open_connection(host, port)

    logging.info(f"{host}:{port} Send CAP msg")
    cap_msg = "CAP REQ : no-history\r\n"
    writer.write(cap_msg.encode("utf-8"))

    logging.info(f"{host}:{port} Send NICK msg")
    nick_msg = f"NICK {nick}\r\n"
    writer.write(nick_msg.encode("utf-8"))

    logging.info(f"{host}:{port} Send CAP END msg")
    cap_end_msg = "CAP END\r\n"
    writer.write(cap_end_msg.encode("utf-8"))

    logging.info(f"{host}:{port} Send JOIN msg for {chan}")
    join_msg = f"JOIN {chan}\r\n"
    writer.write(join_msg.encode("utf-8"))

    logging.info(f"{host}:{port} Listening to channel: {chan}")
    while True:
        msg = await reader.read(1024)
        msg = msg.decode("utf8")
        if not msg:
            continue

        command = msg.split(" ")[1]

        if command == "PRIVMSG":
            msg_title = msg.split(" ")[3][1:].rstrip()
            if not msg_title:
                logging.info("Got empty PRIVMSG, ignoring")
                continue

            if msg_title == "!start":
                topics = CHANS[chan]["topics"]
                reply = f"PRIVMSG {chan} :Meeting started\r\n"
                writer.write(reply.encode("utf-8"))
                await writer.drain()

                reply = f"PRIVMSG {chan} :Topics:\r\n"
                writer.write(reply.encode("utf-8"))
                await writer.drain()

                for i, topic in enumerate(topics):
                    reply = f"PRIVMSG {chan} :1. {topic}\r\n"
                    writer.write(reply.encode("utf-8"))
                    await writer.drain()

                if len(topics) > 0:
                    cur_topic = topics.pop(0)
                    reply = f"PRIVMSG {chan} :Current topic: {cur_topic}\r\n"
                else:
                    reply = f"PRIVMSG {chan} :No further topics\r\n"

                CHANS[chan]["topics"] = topics

                writer.write(reply.encode("utf-8"))
                await writer.drain()
                continue

            if msg_title == "!end":
                reply = f"PRIVMSG {chan} :Meeting ended\r\n"
                writer.write(reply.encode("utf-8"))
                await writer.drain()
                continue

            if msg_title == "!topic":
                topic = msg.split(" ", 4)
                if len(topic) != 5:
                    continue
                topic = topic[4].rstrip()
                if topic == "":
                    continue
                topics = CHANS[chan]["topics"]
                topics.append(topic)
                CHANS[chan]["topics"] = topics
                reply = f"PRIVMSG {chan} :Added topic: {topic}\r\n"
                writer.write(reply.encode("utf-8"))
                await writer.drain()
                continue

            if msg_title == "!list":
                topics = CHANS[chan]["topics"]
                if len(topics) == 0:
                    reply = f"PRIVMSG {chan} :No set topics\r\n"
                else:
                    reply = f"PRIVMSG {chan} :Topics:\r\n"
                writer.write(reply.encode("utf-8"))
                await writer.drain()

                for i, topic in enumerate(topics):
                    reply = f"PRIVMSG {chan} :1. {topic}\r\n"
                    writer.write(reply.encode("utf-8"))
                    await writer.drain()

                continue

            if msg_title == "!next":
                topics = CHANS[chan]["topics"]
                if len(topics) == 0:
                    reply = f"PRIVMSG {chan} :No further topics\r\n"
                else:
                    cur_topic = topics.pop(0)
                    CHANS[chan]["topics"] = topics
                    reply = f"PRIVMSG {chan} :Current topic: {cur_topic}\r\n"

                writer.write(reply.encode("utf-8"))
                await writer.drain()
                continue

    return


async def main():
    format = "%(asctime)s: %(message)s"
    logging.basicConfig(format=format, level=logging.INFO, datefmt="%H:%M:%S")

    for i in config["channels"]:
        name = i["name"]
        logging.info(f"Found config for channel {name}")

        # TODO: This will be useful when ircd has a CAP that tells it to
        # give **all** messages to the connected client, no matter if ircd
        # itself has a configured secret or not.
        # This way the ircd itself doesn't have to keep channel secrets, but
        # they can rather only be held by this bot. In turn this means the bot
        # can be deployed with any ircd.
        if i["secret"]:
            logging.info(f"Instantiating NaCl box for {name}")
            sk = b58decode(i["secret"].encode("utf-8"))
            sk = PrivateKey(sk)
            pk = sk.public_key
            box = Box(sk, pk)
        else:
            box = None

        CHANS[name] = {}
        CHANS[name]["box"] = box
        CHANS[name]["topics"] = []

    coroutines = []
    for i in CHANS.keys():
        task = asyncio.create_task(
            channel_listen(config["host"], config["port"], config["nick"], i))
        coroutines.append(task)

    await asyncio.gather(*coroutines)


asyncio.run(main())
