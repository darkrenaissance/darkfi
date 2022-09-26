#!/usr/bin/env python3
import asyncio
import logging
import pickle

from base58 import b58decode
from nacl.public import PrivateKey, Box

from meetbot_cfg import config

# Initialized channels from the configuration
CHANS = {}

# Pickle DB
PICKLE_DB = "meetbot.pickle"


# TODO: while this is nice to support, it would perhaps be better to do it
# all over the same connection rather than opening a socket for each channel.
async def channel_listen(host, port, nick, chan):
    try:
        logging.info("%s: Connecting to %s:%s", chan, host, port)
        reader, writer = await asyncio.open_connection(host, port)

        logging.debug("%s: Send CAP msg", chan)
        msg = "CAP REQ : no-history\r\n"
        writer.write(msg.encode("utf-8"))
        await writer.drain()

        logging.debug("%s: Send NICK msg", chan)
        msg = f"NICK {nick}\r\n"
        writer.write(msg.encode("utf-8"))
        await writer.drain()

        logging.debug("%s: Send CAP END msg", chan)
        msg = "CAP END\r\n"
        writer.write(msg.encode("utf-8"))
        await writer.drain()

        logging.debug("%s: Send JOIN msg", chan)
        msg = f"JOIN {chan}\r\n"
        writer.write(msg.encode("utf-8"))
        await writer.drain()

        logging.info("%s: Listening to channel", chan)
        while True:
            msg = await reader.readline()
            msg = msg.decode("utf8")
            if not msg:
                continue

            split_msg = msg.split(" ")
            command = split_msg[1]
            nick_c = split_msg[0][1:].rsplit("!", 1)[0]
            logging.debug("%s: Recv: %s", chan, msg.rstrip())

            if command == "PRIVMSG":
                msg_title = msg.split(" ")[3][1:].rstrip()
                if not msg_title:
                    logging.info("%s: Recv empty PRIVMSG, ignoring", chan)
                    continue

                if msg_title == "!start":
                    logging.info("%s: Got !start", chan)
                    topics = CHANS[chan]["topics"]
                    reply = f"PRIVMSG {chan} :Meeting started"
                    logging.info("%s: Send: %s", chan, reply)
                    writer.write((reply + "\r\n").encode("utf-8"))
                    await writer.drain()

                    if len(topics) == 0:
                        reply = f"PRIVMSG {chan} :No topics"
                        logging.info("%s: Send: %s", chan, reply)
                        writer.write((reply + "\r\n").encode("utf-8"))
                        await writer.drain()
                        continue

                    reply = f"PRIVMSG {chan} :Topics:"
                    logging.info("%s: Send: %s", chan, reply)
                    writer.write((reply + "\r\n").encode("utf-8"))
                    await writer.drain()

                    for i, topic in enumerate(topics):
                        reply = f"PRIVMSG {chan} :{i+1}. {topic}"
                        logging.info("%s: Send: %s", chan, reply)
                        writer.write((reply + "\r\n").encode("utf-8"))
                        await writer.drain()

                    cur_topic = topics.pop(0)
                    reply = f"PRIVMSG {chan} :Current topic: {cur_topic}\r\n"
                    CHANS[chan]["topics"] = topics
                    writer.write(reply.encode("utf-8"))
                    await writer.drain()
                    continue

                if msg_title == "!end":
                    logging.info("%s: Got !end", chan)
                    reply = f"PRIVMSG {chan} :Meeting ended"
                    logging.info("%s: Send: %s", chan, reply)
                    writer.write((reply + "\r\n").encode("utf-8"))
                    await writer.drain()
                    continue

                if msg_title == "!topic":
                    logging.info("%s: Got !topic", chan)
                    topic = msg.split(" ", 4)

                    if len(topic) != 5:
                        logging.debug("%s: Topic msg len not 5, skipping",
                                      chan)
                        continue

                    topic = topic[4].rstrip() + f" (by {nick_c})"

                    if topic == "":
                        logging.debug("%s: Topic message empty, skipping",
                                      chan)
                        continue

                    topics = CHANS[chan]["topics"]
                    if topic not in topics:
                        topics.append(topic)
                        CHANS[chan]["topics"] = topics
                        logging.debug("%s: Appended topic to channel topics",
                                      chan)
                        reply = f"PRIVMSG {chan} :Added topic: {topic}"
                        logging.info("%s: Send: %s", chan, reply)
                    else:
                        logging.debug("%s: Topic already in list of topics",
                                      chan)
                        reply = f"PRIVMSG {chan} :Topic already in list"
                        logging.info("%s: Send: %s", chan, reply)

                    writer.write((reply + "\r\n").encode("utf-8"))
                    await writer.drain()
                    continue

                if msg_title == "!list":
                    logging.info("%s: Got !list", chan)
                    topics = CHANS[chan]["topics"]
                    if len(topics) == 0:
                        reply = f"PRIVMSG {chan} :No topics"
                    else:
                        reply = f"PRIVMSG {chan} :Topics:"

                    logging.info("%s: Send: %s", chan, reply)
                    writer.write((reply + "\r\n").encode("utf-8"))
                    await writer.drain()

                    for i, topic in enumerate(topics):
                        reply = f"PRIVMSG {chan} :{i+1}. {topic}"
                        logging.info("%s: Send: %s", chan, reply)
                        writer.write((reply + "\r\n").encode("utf-8"))
                        await writer.drain()

                    continue

                if msg_title == "!next":
                    logging.info("%s: Got !next", chan)
                    topics = CHANS[chan]["topics"]
                    if len(topics) == 0:
                        reply = f"PRIVMSG {chan} :No further topics"
                    else:
                        cur_topic = topics.pop(0)
                        CHANS[chan]["topics"] = topics
                        reply = f"PRIVMSG {chan} :Current topic: {cur_topic}"

                    logging.info("%s: Send: %s", chan, reply)
                    writer.write((reply + "\r\n").encode("utf-8"))
                    await writer.drain()
                    continue

    except KeyboardInterrupt:
        return
    except ConnectionRefusedError:
        logging.warning("%s: Connection refused, trying again in 3s...", chan)
        await asyncio.sleep(3)
        await channel_listen(host, port, nick, chan)
    except Exception as e:
        logging.error("EXCEPTION: %s", e)
        logging.warn("%s: Connection interrupted. Reconnecting in 3s...", chan)
        await asyncio.sleep(3)
        await channel_listen(host, port, nick, chan)


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
        logging.info("Did not find pickle database")

    for i in config["channels"]:
        name = i["name"]
        logging.info("Found config for channel %s", name)

        # TODO: This will be useful when ircd has a CAP that tells it to
        # give **all** messages to the connected client, no matter if ircd
        # itself has a configured secret or not.
        # This way the ircd itself doesn't have to keep channel secrets, but
        # they can rather only be held by this bot. In turn this means the bot
        # can be deployed with any ircd.
        if i["secret"]:
            logging.info("Instantiating NaCl box for %s", name)
            secret = b58decode(i["secret"].encode("utf-8"))
            secret = PrivateKey(secret)
            public = secret.public_key
            box = Box(secret, public)
        else:
            box = None

        if not CHANS.get(name):
            CHANS[name] = {}

        if not CHANS[name].get("topics"):
            CHANS[name]["topics"] = []

        CHANS[name]["box"] = box

    coroutines = []
    for i in CHANS.keys():
        logging.debug("Creating async task for %s", i)
        task = asyncio.create_task(
            channel_listen(config["host"], config["port"], config["nick"], i))
        coroutines.append(task)

    await asyncio.gather(*coroutines)


if __name__ == "__main__":
    from sys import argv
    DBG = bool(len(argv) == 2 and argv[1] == "-v")

    try:
        asyncio.run(main(debug=DBG))
    except KeyboardInterrupt:
        print("\rCaught ^C, saving pickle and exiting")

    with open(PICKLE_DB, "wb") as fdesc:
        pickle.dump(CHANS, fdesc, protocol=pickle.HIGHEST_PROTOCOL)
