import asyncio
import traceback
import html.parser
import irc
import signal
from telegram import Bot
from io import StringIO

TOKEN = "..."
TOKEN_TEST = "..."
SERVER = "127.0.0.1"
PORT = 6645
CHANNELS = ["#dev","#memes","#philosophy","#markets","#math","#random",]
BOTNICK = "tgbridge"

def signal_handler(sig, frame):
    print("Caught termination signal, cleaning up and exiting...")
    ircc.disconnect(SERVER, PORT)
    print("Shut down successfully")
    exit(0)

signal.signal(signal.SIGINT, signal_handler)
signal.signal(signal.SIGTERM, signal_handler)

class HTMLTextExtractor(html.parser.HTMLParser):
    def __init__(self):
        super(HTMLTextExtractor, self).__init__()
        self.reset()
        self.strict = False
        self.convert_charrefs= True
        self.text = StringIO()

    def handle_data(self, d):
        self.text.write(d)

    def get_text(self):
        return self.text.getvalue()

def html_to_text(html):
    s = HTMLTextExtractor()
    s.feed(html)
    return s.get_text()

def append_log(channel, username, message):
    with open(f"/srv/http/log/{channel}.txt", "a") as fd:
        fd.write(f"<{username}> {message}\n")
    with open(f"/srv/http/log/all.txt", "a") as fd:
        fd.write(f"{channel} <{username}> {message}\n")


ircc = irc.IRC()
ircc.connect(SERVER, PORT, CHANNELS, BOTNICK)

async def main():
    while True:
        text = ircc.get_response()
        # print(text)
        if not len(text) > 0:
            continue
        text_list = text.split(' ')
        nick = text_list[0].split('!')[0][1:]
        if text_list[1] == "PRIVMSG":
            channel = text_list[2]
            message = ' '.join(text_list[3:])
            # remove the prefix
            message = message[1:]
            # ignore test msgs
            if message.lower() == "test" or message.lower() == "echo":
                continue
            if nick == "testbot":
                continue

            # Strip all HTML tags
            #message = html_to_text(message)
            message = message.replace("<", "&lt;")
            message = message.replace(">", "&gt;")
            # Limit line lengths
            message = message[:300]

            # Left pad nickname
            nick = nick.replace("<", "&lt;")
            nick = nick.replace(">", "&gt;")
            nick = nick.rjust(12)

            # Limit line lengths
            message = message[:300]
            msg = f"<code>{channel} {nick} |</code> {message}"

            # print(msg)

            append_log(channel, nick, message)

            # Keep retrying until the fucker is sent
            while True:
                try:
                    async with Bot(TOKEN) as bot:
                        await bot.send_message("@darkfi_darkirc", msg,
                                            parse_mode="HTML",
                                            disable_notification=True,
                                            disable_web_page_preview=True)
                    break
                #except telegram.error.BadRequest:
                #    pass
                except:
                    print(channel, msg)
                    print(traceback.format_exc())
                    await asyncio.sleep(3)

asyncio.run(main())
