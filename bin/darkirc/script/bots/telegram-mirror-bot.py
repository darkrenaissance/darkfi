import asyncio, json, traceback
import html.parser
import telegram
from telegram import Bot, Update
from telegram.ext import Application, CommandHandler, ContextTypes, MessageHandler, filters
from io import StringIO

TOKEN = "..."
TOKEN_TEST = "..."

class IrcBot:
 
    async def connect(self, server, port):
        self._reader, self._writer = await asyncio.open_connection(server, port)
        self._send("USER tgbridge 0 * :tgbridge")
        self._send("NICK tgbridge")
        await self._recv()
        self._send("CAP REQ :no-history")
        await self._recv()
        self._send("CAP END")

    def _send(self, msg):
        msg += "\r\n"
        self._writer.write(msg.encode())

    async def _recv(self):
        message = await self._reader.readline()
        message = message.decode()
        return message.removesuffix("\r\n")

    async def get_message(self):
        while True:
            line = await self._recv()
            print(f"Received line: {line}")
            tokens = line.split(" ")
            if len(tokens) < 2 or tokens[1] != "PRIVMSG":
                continue

            assert tokens[0][0] == ":"
            username = tokens[0].split("!")[0][1:]
            channel = tokens[2]

            message = ":".join(line.split(":")[2:])

            return (username, channel, message)

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

async def main():
    irc = IrcBot()
    await irc.connect("localhost", 6667)

    while True:
        nick, channel, message = await irc.get_message()

        if message.lower() == "test":
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

