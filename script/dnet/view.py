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

import urwid
import asyncio
import time
from scroll import ScrollBar, Scrollable

event_loop = asyncio.get_event_loop()

class LeftList(urwid.ListBox):
    def focus_next(self):
        try: 
            self.body.set_focus(self.body.get_next(self.body.get_focus()[1])[1])
        except:
            pass
    def focus_previous(self):
        try: 
            self.body.set_focus(self.body.get_prev(self.body.get_focus()[1])[1])
        except:
            pass            

    def load_info(self):
        return InfoWidget(self)

class ServiceView(urwid.WidgetWrap):
    def __init__(self):
        test = urwid.Text("1")
        super().__init__(test)
        self._w = urwid.AttrWrap(self._w, None)
        self.update_w()

    def selectable(self):
        return True

    def keypress(self, size, key):
        if key in ('q'):
            raise urwid.ExitMainLoop()
        return key

    def update_w(self):
        self._w.focus_attr = 'line'

    def name(self):
        return "ServiceView"

class SessionView(urwid.WidgetWrap):
    def __init__(self):
        test = urwid.Text("2")
        super().__init__(test)
        self._w = urwid.AttrWrap(self._w, None)
        self.update_w()

    def selectable(self):
        return True

    def keypress(self, size, key):
        if key in ('q'):
            raise urwid.ExitMainLoop()
        return key

    def update_w(self):
        self._w.focus_attr = 'line'

    def name(self):
        return "SessionView"

class ConnectView(urwid.WidgetWrap):
    def __init__(self):
        test = urwid.Text("3")
        super().__init__(test)
        self._w = urwid.AttrWrap(self._w, None)
        self.update_w()

    def selectable(self):
        return True

    def keypress(self, size, key):
        if key in ('q'):
            raise urwid.ExitMainLoop()
        return key

    def update_w(self):
        self._w.focus_attr = 'line'

    def name(self):
        return "ConnectView"

class Dnetview():
    palette = [
              ('body','light gray','black', 'standout'),
              ("line","dark cyan","black","standout"),
              ]

    def __init__(self, data=None):
        info_text = urwid.Text("")
        self.pile = urwid.Pile([info_text])
        scroll = ScrollBar(Scrollable(self.pile))
        rightbox = urwid.LineBox(scroll)
        
        widget = ServiceView()
        widget2 = SessionView()
        widget3 = ConnectView()

        listbox_content = [widget, widget2, widget3]
        self.listbox = LeftList(urwid.SimpleListWalker(listbox_content))
        leftbox = urwid.LineBox(self.listbox)

        columns = urwid.Columns([leftbox, rightbox], focus_column=0)
        self.view = urwid.Frame(urwid.AttrWrap( columns, 'body' ))

    def main(self):
        event_loop.create_task(self.render_info())
        loop = urwid.MainLoop(self.view, self.palette,
            event_loop=urwid.AsyncioEventLoop(loop=event_loop))
        loop.run()

    async def render_info(self):
        while True:
            await asyncio.sleep(0.1)
            self.pile.contents.clear()
            focus_w = self.listbox.get_focus()
            match focus_w[0].name():
                case "ServiceView":
                    self.pile.contents.append((urwid.Text("1"), self.pile.options()))
                case "SessionView":
                    self.pile.contents.append((urwid.Text("2"), self.pile.options()))
                case "ConnectView":
                    self.pile.contents.append((urwid.Text("3"), self.pile.options()))
    
if __name__ == '__main__':
    dnet = Dnetview()
    dnet.main()








