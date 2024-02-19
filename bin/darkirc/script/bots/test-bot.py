# -*- coding: utf-8 -*-

import irc

## IRC Config
server = "127.0.0.1"
port = 11007
channels = ["#dev", "#memes", "#philosophy", "#markets", "#math", "#random", "#test"]
botnick = "testbot"
ircc = irc.IRC()
ircc.connect(server, port, channels, botnick)

while True:
    text = ircc.get_response().strip()
    if not len(text) > 0:
        continue
    # print(text)
    text_list = text.split(' ')
    #print(text_list)
    if text_list[1] == "PRIVMSG":
        channel = text_list[2]
        msg = ' '.join(text_list[3:]).strip()
        bot_msg = text.split(':')[-1].strip()
        if bot_msg.lower() == "test" or bot_msg.lower() == "echo":
            ircc.send(channel, f"{bot_msg} back")
            continue
