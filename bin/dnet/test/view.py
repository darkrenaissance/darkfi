# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2023 Dyne.org foundation
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as
# published by the Free Software Foundation, either version 3 of the
# License, or (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.

import urwid, asyncio

class Demo:
    palette = [('body','light gray','black', 'standout')]

    def __init__(self, data=None):
        self.listbox_content = []
        self.listwalker = urwid.SimpleListWalker(self.listbox_content)
        self.list = urwid.ListBox(self.listwalker)
        self.ui = urwid.Frame(urwid.AttrWrap( self.list, 'body' ))

        self.ev = asyncio.new_event_loop()
        asyncio.set_event_loop(self.ev)

    def main(self):
        self.loop = urwid.MainLoop(self.ui, self.palette,
            event_loop=urwid.AsyncioEventLoop(loop=self.ev),
                                   unhandled_input=self.unhandled_input)
        self.ev.create_task(self.say_hello())
        self.loop.run()

    def unhandled_input(self, k):
        if k in ('q','Q'):
            raise urwid.ExitMainLoop()


    async def say_hello(self):
        while True:
            await asyncio.sleep(0.1)
            self.listwalker.contents.append(urwid.Text("hello"))
    

if __name__ == '__main__':
    demo = Demo()
    demo.main()

