/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

# -*- coding: utf-8 -*-

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

while True:
    darkirc_text = darkirc.get_response()
    if not len(darkirc_text) > 0:
        continue
    
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
