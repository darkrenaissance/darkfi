# -*- coding: utf-8 -*-

import signal
import irc
import argparse

# parse arguments
parser = argparse.ArgumentParser(description='IRC bot to mirror msgs from a server to another')
parser.add_argument('--server-from',default='127.0.0.1', help='First IRC server')
parser.add_argument('--port-from', default=22024, type=int, help='port of the first IRC server')
parser.add_argument('--server-to',default='127.0.0.1', help='Second IRC server')
parser.add_argument('--port-to', default=22028, type=int, help='port of the second IRC server')
parser.add_argument('--channels', 
                    default=["#dev", "#memes", "#philosophy", "#markets", "#math", "#random", "#test"], 
                    nargs='+', help='channels to join')
parser.add_argument('--nickname', default="mirror-bot", help='bot nickname in IRC')

args = parser.parse_args()

botnick = args.nickname

## Server 1
darkirc_server = args.server_from
darkirc_port = args.port_from
darkirc_channels = args.channels

darkirc = irc.IRC()
darkirc.connect(darkirc_server, darkirc_port, darkirc_channels, botnick)

## Server 2
ircd_server = args.server_to
ircd_port = args.port_to
ircd_channels = args.channels

ircd = irc.IRC()
ircd.connect(ircd_server, ircd_port, ircd_channels, botnick)

def signal_handler(sig, frame):
    print("Caught termination signal, cleaning up and exiting...")
    ircd.disconnect(args.server_to, args.port_to)
    darkirc.disconnect(args.server_from, args.port_from)
    print("Shut down successfully")
    exit(0)

signal.signal(signal.SIGINT, signal_handler)


while True:
    darkirc_text = darkirc.get_response()
    if not len(darkirc_text.strip()) > 0:
        print("Error: disconnected from server")
        exit(-1)
    
    print(darkirc_text)
    text_list = darkirc_text.split(' ')
    command = text_list[1]
    nick_handle = text_list[0]

    if command == "PRIVMSG":
        nickname = nick_handle[nick_handle.find(':')+1 : nick_handle.find('!')]
        channel = text_list[2]
        msg = ' '.join(text_list[3:])
        message = msg[msg.find(':')+1:].rstrip()
        
        ircd.send(channel, f"<{nickname}>: {message}")
