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

class MyListBox(urwid.ListBox):
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

class ServiceWidget(urwid.WidgetWrap):
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
        return "ServiceWidget"

class SessionWidget(urwid.WidgetWrap):
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
        return "SessionWidget"

class ConnectWidget(urwid.WidgetWrap):
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
        return "ConnectWidget"

def main():
    palette = [
              ('body','light gray','black', 'standout'),
              ("line","dark cyan","black","standout"),
              ]

    info_text = urwid.Text("")
    pile = urwid.Pile([info_text])
    scroll = ScrollBar(Scrollable(pile))
    rightbox = urwid.LineBox(scroll)
    
    widget = ServiceWidget()
    widget2 = SessionWidget()
    widget3 = ConnectWidget()
    listbox_content = [widget, widget2, widget3]
    listbox = MyListBox(urwid.SimpleListWalker(listbox_content))
    leftbox = urwid.LineBox(listbox)

    columns = urwid.Columns([leftbox, rightbox], focus_column=0)
    view = urwid.Frame(urwid.AttrWrap( columns, 'body' ))

    event_loop.create_task(render_info(pile, listbox))

    loop = urwid.MainLoop(view, palette,
        event_loop=urwid.AsyncioEventLoop(loop=event_loop),
                               )

    loop.run()

async def render_info(pile, listbox):
    while True:
        await asyncio.sleep(0.1)
        pile.contents.clear()
        focus_w = listbox.get_focus()
        match focus_w[0].name():
            case "ServiceWidget":
                pile.contents.append((urwid.Text("1"), pile.options()))
            case "SessionWidget":
                pile.contents.append((urwid.Text("2"), pile.options()))
            case "ConnectWidget":
                pile.contents.append((urwid.Text("3"), pile.options()))

if __name__ == "__main__":
   main()








