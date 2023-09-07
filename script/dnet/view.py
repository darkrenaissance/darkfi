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
import logging
import asyncio

from scroll import ScrollBar, Scrollable
from model import NodeInfo

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
    def __init__(self, info):
        test = urwid.Text(f"{info}")
        super().__init__(test)
        self._w = urwid.AttrWrap(self._w, None)
        self.update_w()

    def selectable(self):
        return True

    def keypress(self, size, key):
        #if key in ('q'):
        #    raise urwid.ExitMainLoop()
        return key

    def update_w(self):
        self._w.focus_attr = 'line'

    def name(self):
        return "ServiceView"

class SessionView(urwid.WidgetWrap):
    def __init__(self, info):
        test = urwid.Text(f"{info}")
        super().__init__(test)
        self._w = urwid.AttrWrap(self._w, None)
        self.update_w()

    def selectable(self):
        return True

    def keypress(self, size, key):
        #if key in ('q'):
        #    raise urwid.ExitMainLoop()
        return key

    def update_w(self):
        self._w.focus_attr = 'line'

    def name(self):
        return "SessionView"

class ConnectView(urwid.WidgetWrap):
    def __init__(self, info):
        test = urwid.Text(f"{info}")
        super().__init__(test)
        self._w = urwid.AttrWrap(self._w, None)
        self.update_w()

    def selectable(self):
        return True

    def keypress(self, size, key):
        #if key in ('q'):
        #    raise urwid.ExitMainLoop()
        return key

    def update_w(self):
        self._w.focus_attr = 'line'

    def name(self):
        return "ConnectView"

class View():
    palette = [
              ('body','light gray','black', 'standout'),
              ("line","dark cyan","black","standout"),
              ]

    def __init__(self, data=NodeInfo):
        #logging.debug(f"dnetview init {data}")

        info_text = urwid.Text("")
        self.pile = urwid.Pile([info_text])
        scroll = ScrollBar(Scrollable(self.pile))
        rightbox = urwid.LineBox(scroll)
        
        self.service_info = urwid.Text("")
        widget = ServiceView(self.service_info)

        self.session_info = urwid.Text("")
        widget2 = SessionView(self.session_info)

        self.connect_info = urwid.Text("")
        widget3 = ConnectView(self.connect_info)

        self.listbox_content = [widget, widget2, widget3]
        self.listbox = LeftList(urwid.SimpleListWalker(self.listbox_content))
        leftbox = urwid.LineBox(self.listbox)

        columns = urwid.Columns([leftbox, rightbox], focus_column=0)
        self.ui = urwid.Frame(urwid.AttrWrap( columns, 'body' ))

    async def update_view(self, data=NodeInfo):
        while True:
            await asyncio.sleep(0.1)
            self.service_info = urwid.Text("")
       
    async def render_info(self, channels):
        while True:
            await asyncio.sleep(0.1)
            self.pile.contents.clear()
            focus_w = self.listbox.get_focus()
            match focus_w[0].name():
                case "ServiceView":
                    self.pile.contents.append((urwid.Text(f""), self.pile.options()))
                case "SessionView":
                    self.pile.contents.append((urwid.Text("2"), self.pile.options()))
                case "ConnectView":
                    self.pile.contents.append((urwid.Text("3"), self.pile.options()))
