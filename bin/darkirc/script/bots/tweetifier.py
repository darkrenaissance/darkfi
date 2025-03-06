# -*- coding: utf-8 -*-

import re
import signal
import irc
from tweety import Twitter
from urllib.parse import urlparse

## IRC Config
server = "127.0.0.1"
port = 11069
channels = ["#test", "#test1"]
botnick = "tweetifier"
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
    print(text)
    text_list = text.split(' ')
    if text_list[1] == "PRIVMSG":
        channel = text_list[2]
        msg = ' '.join(text_list[3:])
        url = re.findall(r'(https?://[^\s]+)', msg)
        
        for i in url:
            parsed_url = urlparse(i)
            if str(parsed_url.path).endswith("/"):
                tweetId = str(parsed_url.path)[:-1].split("/")[-1]
            else:
                tweetId = str(parsed_url.path).split("/")[-1]
            print(f"tweet id: {tweetId}")
            if not (parsed_url.netloc.lower() in ['twitter.com','t.co', 'x.com'] and parsed_url.scheme == 'https'):
                continue
            app = Twitter("session")
            try:
                tweet_text = app.tweet_detail(tweetId)
            except:
                print("Error: The Identifier provided of the tweet is either invalid or the tweet is private")
                continue

            author_name = tweet_text.author.name
            screen_name = tweet_text.author.screen_name

            tt = tweet_text.text.split('\n')
            tweet_msg = []
            # remove empty lines from tweet body
            for line in tt:
                if not line.strip():
                    continue
                tweet_msg.append(line)
            tweetify = str(' '.join(tweet_msg))
            if tweetify.startswith("@"):
                tweetify = f"Replying to {tweetify}"
            print(tweetify)

            ircc.send(channel, f"{author_name}(@{screen_name}): {tweetify}")
