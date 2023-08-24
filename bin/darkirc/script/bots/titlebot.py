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
