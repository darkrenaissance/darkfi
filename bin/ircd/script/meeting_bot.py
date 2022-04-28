import asyncio


async def start():
    host = "127.0.0.1"
    port = 11066
    channel = "#dev"
    nickname = "meeting"

    print(f"Start a connection {host}:{port}")
    reader, writer = await asyncio.open_connection(host, port)

    print("Send NICK msg")
    nick_msg = f"NICK {nickname} \r\n"
    writer.write(nick_msg.encode('utf8'))

    print(f"Send JOIN msg: {channel}")
    join_msg = f"JOIN {channel} \r\n"
    writer.write(join_msg.encode('utf8'))

    topics = []

    while True:
        msg = await reader.read(350)
        msg = msg.decode('utf8').strip()

        if not msg:
            print("Error: Receive empty msg")
            break

        command = msg.split(" ")[1]

        if command == "PRIVMSG":

            msg_title = msg.split(" ")[3][1:]

            if not msg_title:
                continue

            reply = None

            if msg_title == "#m_start":
                reply = f"PRIVMSG {channel} :meeting started \r\n"
                msg_title = "#m_list"

            if msg_title == "#m_end":
                reply = f"PRIVMSG {channel} :meeting end \r\n"
                topics = []

            if msg_title == "#m_topic":
                topic = msg.split(" ", 4)
                if len(topic) != 5:
                    continue
                topic = topic[4]
                topics.append(topic)
                reply = f"PRIVMSG {channel} :add topic: {topic} \r\n"

            if msg_title == "#m_list":
                tp = " ".join(
                    [f"{i}-{topic}" for i, topic in enumerate(topics, 1)])

                reply = f"PRIVMSG {channel} :topics: {tp} \r\n"

            if msg_title == "#m_next":
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
