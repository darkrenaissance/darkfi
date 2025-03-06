# -*- coding: utf-8 -*-

import re
import signal
import irc
import requests
from bs4 import BeautifulSoup
from urllib.parse import urlparse

## IRC Config
server = "127.0.0.1"
port = 22025
channels = ["#test", "#test1"]
botnick = "website-title"
ircc = irc.IRC()
ircc.connect(server, port, channels, botnick)

def signal_handler(sig, frame):
    print("Caught termination signal, cleaning up and exiting...")
    ircc.disconnect(server, port)
    print("Shut down successfully")
    exit(0)

signal.signal(signal.SIGINT, signal_handler)

while True:
    text = ircc.get_response()
    if not len(text) > 0:
        print("Error: disconnected from server")
        exit(-1)
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
