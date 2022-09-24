#TODO:
# Expand home path

import logging
import threading
import time
import json
import asyncio

def load_config():
    with open('/home/{user}/.config/darkfi/meeting_bot_config.json', 'r') as config:
        data = json.load(config)    
    logging.info(f"Config loaded: {data}")
    return data

def thread_run(host, port, nickname, channel):
    asyncio.run(channel_listen(host, port, nickname, channel))

async def channel_listen(host, port, nickname, channel):
    logging.info(f"Starting listening to channel: {channel['name']}")

    logging.info(f"Start a connection {host}:{port}")
    reader, writer = await asyncio.open_connection(host, port)

    logging.info("Send CAP msg")
    cap_msg = f"CAP REQ : no-history \r\n"
    writer.write(cap_msg.encode('utf8'))

    logging.info("Send NICK msg")
    nick_msg = f"NICK {nickname} \r\n"
    writer.write(nick_msg.encode('utf8'))

    logging.info("Send CAP END msg")
    cap_end_msg = f"CAP END \r\n"
    writer.write(cap_end_msg.encode('utf8'))

    logging.info(f"Send JOIN msg: {channel['name']}")
    join_msg = f"JOIN {channel['name']} \r\n"
    writer.write(join_msg.encode('utf8'))

    topics = []

    logging.info("Start...")
    while True:
        msg = await reader.read(1024)
        msg = msg.decode('utf8').strip()

        if not msg:
            continue

        command = msg.split(" ")[1]

        if command == "PRIVMSG":

            msg_title = msg.split(" ")[3][1:]

            if not msg_title:
                continue

            reply = None

            if msg_title == "!start":
                reply = f"PRIVMSG {channel['name']} :meeting started \r\n"
                msg_title = "!list"

            if msg_title == "!end":
                reply = f"PRIVMSG {channel['name']} :meeting end \r\n"
                topics = []

            if msg_title == "!topic":
                topic = msg.split(" ", 4)
                if len(topic) != 5:
                    continue
                topic = topic[4]
                topics.append(topic)
                reply = f"PRIVMSG {channel['name']} :add topic: {topic} \r\n"

            if msg_title == "!list":
                rep = f"PRIVMSG {channel['name']} :topics: \r\n"
                writer.write(rep.encode('utf8'))

                for i, topic in enumerate(topics, 1):
                    rep = f"PRIVMSG {channel['name']} :{i}-{topic} \r\n"
                    writer.write(rep.encode('utf8'))
                await writer.drain()

            if msg_title == "!next":
                if len(topics) == 0:
                    reply = f"PRIVMSG {channel['name']} :no topics \r\n"
                else:
                    tp = topics.pop(0)
                    reply = f"PRIVMSG {channel['name']} :current topic: {tp} \r\n"

            if reply != None:
                writer.write(reply.encode('utf8'))
                await writer.drain()

        if command == "QUIT":
            break

    writer.close()

if __name__ == "__main__":
    format = "%(asctime)s: %(message)s"
    logging.basicConfig(format=format, level=logging.INFO,
                        datefmt="%H:%M:%S")

    data = load_config()

    threads = list()
    for index in range(len(data['channels'])):
        logging.info(f"Main    : create and start thread {index}.")
        x = threading.Thread(target=thread_run, args=(data['host'], data['port'], data['nickname'], data['channels'][index],))
        threads.append(x)
        x.start()

    for index, thread in enumerate(threads):
        logging.info(f"Main    : before joining thread {index}.")
        thread.join()
        logging.info(f"Main    : thread {index} done")

