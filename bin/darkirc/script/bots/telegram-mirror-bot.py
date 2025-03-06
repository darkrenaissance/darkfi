import asyncio
import traceback
import html.parser
import irc
import signal
from telegram import Bot
from telegram.constants import MessageLimit
from io import StringIO

TOKEN = "..."
TOKEN_TEST = "..."

MAX_CHANNEL_LENGTH = 10
MAX_NICK_LENGTH = 10
MAX_MESSAGE_LENGTH = MessageLimit.MAX_TEXT_LENGTH - (MAX_CHANNEL_LENGTH + MAX_NICK_LENGTH + 16)

SERVER = "127.0.0.1"
PORT = 22025
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
        # print("loop")
        print("text: " + text)
        if not len(text) > 0:
            print("Error: disconnected from server")
            exit(-1)
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
            channel = channel.replace("<", "&lt;")
            channel = channel.replace(">", "&gt;")
            nick = nick.replace("<", "&lt;")
            nick = nick.replace(">", "&gt;")
            message = message.replace("<", "&lt;")
            message = message.replace(">", "&gt;")
    
            # https://www.irchelp.org/protocol/ctcpspec.html
            #
            # "This is used by losers on IRC to simulate 'role playing' games"
            # "Presumably other users on the channel are suitably impressed."
            if message.find("ACTION") == 1:
                message = nick + message[7:]
                nick = "*"

            # append_log(channel, nick, message)

            # Pad and left/right justify channel and nickname
            channel = channel[:MAX_CHANNEL_LENGTH].ljust(MAX_CHANNEL_LENGTH)
            nick = nick[:MAX_NICK_LENGTH].rjust(MAX_NICK_LENGTH)

            # Send messages to Telegram in chunks
            while len(message) > 0:
                string_to_telegram = f"<code>{channel} {nick} |</code> {message[:MAX_MESSAGE_LENGTH]}"

                print("tele: " + string_to_telegram)
                # # Keep retrying until the fucker is sent
                # while True:
                #     try:
                #         async with Bot(TOKEN) as bot:
                #             await bot.send_message("@darkfi_darkirc", string_to_telegram,
                #                                 parse_mode="HTML",
                #                                 disable_notification=True,
                #                                 disable_web_page_preview=True)
                #         break
                #     #except telegram.error.BadRequest:
                #     #    pass
                #     except:
                #         print(channel, string_to_telegram)
                #         print(traceback.format_exc())
                #         await asyncio.sleep(3)

                message = message[MAX_MESSAGE_LENGTH:]

asyncio.run(main())
