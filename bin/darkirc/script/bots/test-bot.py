# -*- coding: utf-8 -*-

import irc
import signal

## IRC Config
server = "127.0.0.1"
port = 11007
channels = ["#dev", "#memes", "#philosophy", "#markets", "#math", "#random", "#test"]
botnick = "testbot"
ircc = irc.IRC()
ircc.connect(server, port, channels, botnick)

def signal_handler(sig, frame):
    print("Caught termination signal, cleaning up and exiting...")
    ircc.disconnect(server, port)
    print("Shut down successfully")
    exit(0)

signal.signal(signal.SIGINT, signal_handler)

while True:
    text = ircc.get_response().strip()
    if not len(text) > 0:
        print("Error: disconnected from server")
        exit(-1)
    # print(text)
    text_list = text.split(' ')
    #print(text_list)
    if text_list[1] == "PRIVMSG":
        channel = text_list[2]
        msg = ' '.join(text_list[3:]).strip()
        bot_msg = text.split(':')[-1].strip()
        if bot_msg.lower() == "test" or bot_msg.lower() == "echo":
            ircc.send(channel, f"{bot_msg} back")
