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
from tweety.bot import Twitter
from urllib.parse import urlparse

## IRC Config
server = "127.0.0.1"
port = 11069
channels = ["#test", "#test1"]
botnick = "tweetifier"
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
            if str(parsed_url.path).endswith("/"):
                tweetId = str(parsed_url.path)[:-1].split("/")[-1]
            else:
                tweetId = str(parsed_url.path).split("/")[-1]
            print(f"tweet id: {tweetId}")
            if not (parsed_url.netloc.lower() in ['twitter.com','t.co', 'x.com'] and parsed_url.scheme == 'https'):
                continue
            app = Twitter()
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
