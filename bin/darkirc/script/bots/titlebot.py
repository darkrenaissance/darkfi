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

import re
import irc
import requests
from bs4 import BeautifulSoup
from urllib.parse import urlparse

## IRC Config
server = "127.0.0.1"
port = 11070
channels = ["#test", "#test1"]
botnick = "website-title"
ircc = irc.IRC()
ircc.connect(server, port, channels, botnick)

while True:
    text = ircc.get_response()
    if not len(text) > 0:
        continue
    print(text)
    text_list = text.split(' ')
    if text_list[1] == "PRIVMSG":
        channel = text_list[2]
        msg = ' '.join(text_list[3:])
        url = re.findall(r'(https?://[^\s]+)', msg)

        for i in url:
            parsed_url = urlparse(i)
            if parsed_url.netloc.lower() in ['twitter.com','t.co', 'x.com'] or parsed_url.scheme != 'https':
                continue
            try:
                reqs = requests.get(i)
            except requests.exceptions.SSLError:
                print("SSLERROR: wrong signature type")
                continue
            soup = BeautifulSoup(reqs.text, 'html.parser')

            try:
                title_text = soup.find('title').get_text()
            except:
                print("Error: Title not found!")
                continue
            title_text = title_text.split('\n')
            title_msg = []
            # remove empty lines from title body
            for line in title_text:
                if not line.strip():
                    continue
                title_msg.append(line)
            title_msg = " ".join(title_msg)
            print(f"Title: {title_msg}")
            ircc.send(channel, f"Title: {title_msg}")
