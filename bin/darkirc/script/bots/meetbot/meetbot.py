#!/usr/bin/env python3
import asyncio
import logging
import pickle
from time import time

from meetbot_cfg import config

# Initialized channels from the configuration
CHANS = {}

# Pickle DB
PICKLE_DB = "meetbot.pickle"


async def main(debug=False):
    global CHANS

    loglevel = logging.DEBUG if debug else logging.INFO
    logfmt = "%(asctime)s [%(levelname)s]\t%(message)s"
    logging.basicConfig(format=logfmt,
                        level=loglevel,
                        datefmt="%Y-%m-%d %H:%M:%S")

    try:
        with open(PICKLE_DB, "rb") as pickle_fd:
            CHANS = pickle.load(pickle_fd)
        logging.info("Loaded pickle database")
    except:
        logging.info("Did not find existing pickle database")

    host = config["host"]
    port = config["port"]
    nick = config["nick"]

    try:
        logging.info("Connecting to %s/%d", host, port)
        reader, writer = await asyncio.open_connection(host, port)

        logging.debug("--> %s/%d: CAP LS 302", host, port)
        msg = "CAP LS 302\r\n"
        writer.write(msg.encode("utf-8"))
        await writer.drain()

        logging.debug("--> %s/%d: NICK %s", host, port, nick)
        msg = f"NICK {nick}\r\n"
        writer.write(msg.encode("utf-8"))
        await writer.drain()

        logging.debug("--> %s/%d: USER %s * 0 %s", host, port, nick, nick)
        msg = f"USER {nick} * 0 :{nick}\r\n"
        writer.write(msg.encode("utf-8"))
        await writer.drain()

        msg = await reader.readline()
        msg = msg.decode("utf-8")
        logging.debug("<-- %s/%d: %s", host, port, msg)

        logging.debug("--> %s/%d: CAP REQ :no-history", host, port)
        msg = "CAP REQ :no-history\r\n"
        writer.write(msg.encode("utf-8"))
        await writer.drain()

        logging.debug("--> %s/%d: CAP REQ :no-autojoin", host, port)
        msg = "CAP REQ :no-autojoin\r\n"
        writer.write(msg.encode("utf-8"))
        await writer.drain()

        msg = await reader.readline()
        msg = msg.decode("utf-8")
        logging.debug("<-- %s/%d: %s", host, port, msg)

        logging.debug("--> %s/%d: CAP END", host, port)
        msg = "CAP END\r\n"
        writer.write(msg.encode("utf-8"))
        await writer.drain()

        msg = await reader.readline()
        msg = msg.decode("utf-8")
        logging.debug("<-- %s/%d: %s", host, port, msg)

        for chan in config["channels"]:
            chan = chan["name"]
            logging.debug("--> %s/%d: JOIN %s", host, port, chan)
            msg = f"JOIN {chan}\r\n"
            writer.write(msg.encode("utf-8"))
            await writer.drain()

        channels = [chan["name"] for chan in config["channels"]]
        logging.info("%s/%d: Listening to channels: %s", host, port, channels)

        elapsed = 0

        while True:
            msg = await reader.readline()
            msg = msg.decode("utf-8")
            if not msg:
                continue

            split_msg = msg.split(" ")
            command = split_msg[1]
            chan = split_msg[2]
            nick_c = split_msg[0][1:].rsplit("!", 1)[0]
            logging.debug("<-- %s/%d: %s", host, port, msg.rstrip())

            if command == "PRIVMSG":
                msg_title = msg.split(" ")[3][1:].rstrip()
                if not msg_title:
                    continue

                if msg_title == "!start":
                    logging.info("%s: Got !start", chan)

                    if not CHANS.get(chan):
                        CHANS[chan] = {}
                        CHANS[chan]["topics"] = {}

                    topics = CHANS[chan]["topics"]

                    reply = f"PRIVMSG {chan} :Meeting started\r\n"
                    writer.write(reply.encode("utf-8"))
                    await writer.drain()

                    if len(topics) == 0:
                        reply = f"PRIVMSG {chan} :No topics\r\n"
                        writer.write(reply.encode("utf-8"))
                        await writer.drain()
                        continue

                    reply = f"PRIVMSG {chan} :Topics:\r\n"
                    writer.write(reply.encode("utf-8"))
                    await writer.drain()

                    for i, topic in enumerate(topics):
                        reply = f"PRIVMSG {chan} :{i+1}. {topic}\r\n"
                        writer.write(reply.encode("utf-8"))
                        await writer.drain()

                    cur_topic = topics.pop(0)
                    reply = f"PRIVMSG {chan} :Current topic: {cur_topic}\r\n"
                    CHANS[chan]["topics"] = topics
                    writer.write(reply.encode("utf-8"))
                    await writer.drain()
                    elapsed = time()
                    continue

                if msg_title == "!end":
                    logging.info("%s: Got !end", chan)
                    reply = f"PRIVMSG {chan} :Elapsed time: {round((time() - elapsed)/60, 1)} min\r\n"
                    elapsed = 0
                    writer.write(reply.encode("utf-8"))
                    await writer.drain()
                    reply = f"PRIVMSG {chan} :Meeting ended\r\n"
                    writer.write(reply.encode("utf-8"))
                    await writer.drain()
                    continue

                if msg_title == "!topic":
                    logging.info("%s: Got !topic", chan)
                    topic = msg.split(" ", 4)

                    if len(topic) != 5:
                        continue

                    topic = topic[4].rstrip() + f" (by {nick_c})"

                    if topic == "":
                        continue

                    if not CHANS.get(chan):
                        CHANS[chan] = {}
                        CHANS[chan]["topics"] = {}

                    topics = CHANS[chan]["topics"]
                    if topic not in topics:
                        topics.append(topic)
                        CHANS[chan]["topics"] = topics
                        reply = f"PRIVMSG {chan} :Added topic: {topic}\r\n"
                    else:
                        reply = f"PRIVMSG {chan} :Topic already in list\r\n"

                    writer.write(reply.encode("utf-8"))
                    await writer.drain()
                    continue

                if msg_title == "!deltopic":
                    logging.info("%s: Got !deltopic", chan)

                    if not CHANS.get(chan):
                        CHANS[chan] = {}
                        CHANS[chan]["topics"] = {}

                    topics = CHANS[chan]["topics"]

                    if len(topics) == 0:
                        reply = f"PRIVMSG {chan} :No topics\r\n"
                        writer.write(reply.encode("utf-8"))
                        await writer.drain()
                        continue

                    try:
                        topic = msg.split(" ", 4)
                        topic = int(topic[4].rstrip())
                        del topics[topic-1]
                        CHANS[chan]["topics"] = topics
                        reply = f"PRIVMSG {chan} :Removed topic {topic}\r\n"
                        writer.write(reply.encode("utf-8"))
                        await writer.drain()
                    except:
                        reply = f"PRIVMSG {chan} :Topic not found\r\n"
                        writer.write(reply.encode("utf-8"))
                        await writer.drain()

                    continue

                if msg_title == "!list":
                    logging.info("%s: Got !list", chan)

                    if not CHANS.get(chan):
                        CHANS[chan] = {}
                        CHANS[chan]["topics"] = []

                    topics = CHANS[chan]["topics"]
                    if len(topics) == 0:
                        reply = f"PRIVMSG {chan} :No topics\r\n"
                    else:
                        reply = f"PRIVMSG {chan} :Topics:\r\n"

                    writer.write(reply.encode("utf-8"))
                    await writer.drain()

                    for i, topic in enumerate(topics):
                        reply = f"PRIVMSG {chan} :{i+1}. {topic}\r\n"
                        writer.write(reply.encode("utf-8"))
                        await writer.drain()

                    continue

                if msg_title == "!next":
                    logging.info("%s: Got !next", chan)

                    if not CHANS.get(chan):
                        CHANS[chan] = {}
                        CHANS[chan]["topics"] = []

                    topics = CHANS[chan]["topics"]

                    reply = f"PRIVMSG {chan} :Elapsed time: {round((time() - elapsed)/60, 1)}, min\r\n"
                    writer.write(reply.encode("utf-8"))
                    await writer.drain()
                    elapsed = time()

                    if len(topics) == 0:
                        reply = f"PRIVMSG {chan} :No further topics\r\n"
                    else:
                        cur_topic = topics.pop(0)
                        CHANS[chan]["topics"] = topics
                        reply = f"PRIVMSG {chan} :Current topic: {cur_topic}\r\n"

                    writer.write(reply.encode("utf-8"))
                    await writer.drain()
                    continue

    except KeyboardInterrupt:
        return
    except ConnectionRefusedError:
        logging.error("%s/%d: Connection refused", host, port)
        return


if __name__ == "__main__":
    from sys import argv
    debug = "-v" in argv

    try:
        asyncio.run(main(debug=debug))
    except KeyboardInterrupt:
        print("\rCaught ^C, saving pickle and exiting")

    with open(PICKLE_DB, "wb") as fdesc:
        pickle.dump(CHANS, fdesc, protocol=pickle.HIGHEST_PROTOCOL)
