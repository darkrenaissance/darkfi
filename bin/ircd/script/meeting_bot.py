import asyncio


async def start():
    host = "127.0.0.1"
    port = 6667
    channel = "#dev"
    nickname = "meeting_bot"

    print(f"Start a connection {host}:{port}")
    reader, writer = await asyncio.open_connection(host, port)

    print("Send CAP msg")
    cap_msg = f"CAP REQ : no-history \r\n"
    writer.write(cap_msg.encode('utf8'))

    print("Send NICK msg")
    nick_msg = f"NICK {nickname} \r\n"
    writer.write(nick_msg.encode('utf8'))

    print("Send CAP END msg")
    cap_end_msg = f"CAP END \r\n"
    writer.write(cap_end_msg.encode('utf8'))

    print(f"Send JOIN msg: {channel}")
    join_msg = f"JOIN {channel} \r\n"
    writer.write(join_msg.encode('utf8'))

    topics = []

    print("Start...")
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
                reply = f"PRIVMSG {channel} :meeting started \r\n"
                msg_title = "!list"

            if msg_title == "!end":
                reply = f"PRIVMSG {channel} :meeting end \r\n"
                topics = []

            if msg_title == "!topic":
                topic = msg.split(" ", 4)
                if len(topic) != 5:
                    continue
                topic = topic[4]
                topics.append(topic)
                reply = f"PRIVMSG {channel} :add topic: {topic} \r\n"

            if msg_title == "!list":
                rep = f"PRIVMSG {channel} :topics: \r\n"
                writer.write(rep.encode('utf8'))

                for i, topic in enumerate(topics, 1):
                    rep = f"PRIVMSG {channel} :{i}-{topic} \r\n"
                    writer.write(rep.encode('utf8'))
                await writer.drain()

            if msg_title == "!next":
                if len(topics) == 0:
                    reply = f"PRIVMSG {channel} :no topics \r\n"
                else:
                    tp = topics.pop(0)
                    reply = f"PRIVMSG {channel} :current topic: {tp} \r\n"

            if reply != None:
                writer.write(reply.encode('utf8'))
                await writer.drain()

        if command == "QUIT":
            break

    writer.close()

asyncio.run(start())
