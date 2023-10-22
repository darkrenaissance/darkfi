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
import datetime as dt

from scroll import ScrollBar, Scrollable
from model import Model

#----------------------------------------------------------------------
# TODO: 
#   * create a dictionary that stores:
#   * channel[id] = index
#   * index = listwalker.contents[i]
#   * sort data by ID, constantly update listwalker_contents[i]
#   * if it's a null id, render empty info
# -------------------------------------------------------------------

event_loop = asyncio.get_event_loop()


class LeftList(urwid.ListBox):

    def focus_next(self):
        try: 
            self.body.set_focus(self.body.get_next(
                self.body.get_focus()[1])[1])
        except:
            pass

    def focus_previous(self):
        try: 
            self.body.set_focus(self.body.get_prev(
                self.body.get_focus()[1])[1])
        except:
            pass            


class NodeView(urwid.WidgetWrap):

    def __init__(self, info):
        self.name = info
        self.text = urwid.Text(f"{self.name}")
        super().__init__(self.text)
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

    def get_widget(self):
        return "NodeView"

    def get_name(self):
        return self.name

class ConnectView(urwid.WidgetWrap):

    def __init__(self, node, kind):
        self.name = (f"{node}", f"{kind}")
        self.text = urwid.Text(f"  {kind}")
        super().__init__(self.text)
        self._w = urwid.AttrWrap(self._w, None)
        self.update_w()

    def selectable(self):
        return True

    def keypress(self, size, key):
        return key

    def update_w(self):
        self._w.focus_attr = 'line'

    def get_widget(self):
        return "ConnectView"

    def get_name(self):
        return self.name

class SlotView(urwid.WidgetWrap):

    def __init__(self, node, num, info):
        self.num = num
        self.name = (f"{node}", f"{num}")
        #self.name = info[0]
        self.addr = info
        if len(num) == 1:
            self.text = urwid.Text(f"    {num}: {self.addr}")
        else:
            self.text = urwid.Text(f"    {self.addr}")
        super().__init__(self.text)
        self._w = urwid.AttrWrap(self._w, None)
        self.update_w()

    def selectable(self):
        return True

    def keypress(self, size, key):
        return key

    def update_w(self):
        self._w.focus_attr = 'line'

    def get_widget(self):
        return "SlotView"

    def get_name(self):
        return self.name

    def get_addr(self):
        return self.addr


class View():
    palette = [
              ('body','light gray','default', 'standout'),
              ('line','dark cyan','default','standout'),
              ]

    def __init__(self, model):
        self.model = model
        info_text = urwid.Text("")
        self.pile = urwid.Pile([info_text])
        scroll = ScrollBar(Scrollable(self.pile))
        rightbox = urwid.LineBox(scroll)
        self.listbox_content = []
        self.listwalker = urwid.SimpleListWalker(self.listbox_content)
        self.list = LeftList(self.listwalker)
        leftbox = urwid.LineBox(self.list)
        columns = urwid.Columns([leftbox, rightbox], focus_column=0)
        self.ui = urwid.Frame(urwid.AttrWrap( columns, 'body' ))


    async def update_view(self):
        online = []
        while True:
            await asyncio.sleep(0.1)
            for index, item in enumerate(self.listwalker.contents):
                online.append(item.get_name())

            for node, values in self.model.nodes.items():
                if node in online:
                    continue
                else:
                    widget = NodeView(node)
                    self.listwalker.contents.append(widget)

                    outbounds = values.outbound
                    inbound = values.inbound
                    manual = values.manual
                    seed = values.seed

                    if len(outbounds) != 0:
                        widget = ConnectView(node, "outbound")
                        self.listwalker.contents.append(widget)
                        for i, info in outbounds.items():
                            widget = SlotView(node, i, info)
                            self.listwalker.contents.append(widget)

                    if len(inbound) != 0:
                        widget = ConnectView(node, "inbound")
                        self.listwalker.contents.append(widget)
                        for i, info in inbound.items():
                            widget = SlotView(node, i, info)
                            self.listwalker.contents.append(widget)
                        #logging.debug(len(self.listwalker.contents))
                    if len(seed) != 0:
                        widget = ConnectView(node, "seed")
                        self.listwalker.contents.append(widget)

                    if len(manual) != 0:
                        widget = ConnectView(node, "manual")
                        self.listwalker.contents.append(widget)

            for index, item in enumerate(self.listwalker.contents):
                name = item.get_name()
                if name in self.model.info.event.keys():
                    postfix = name[1]
                    match postfix:
                        case "outbound":
                            # Outhound event info (displayed in render_info())
                            continue
                        case "inbound":
                            continue
                        case _:
                            # Slot event info
                            value = self.model.info.event.get(name)
                            widget = SlotView(node, postfix, value)
                            self.listwalker.contents[index] = widget

    async def render_info(self):
        while True:
            await asyncio.sleep(0.1)
            self.pile.contents.clear()
            focus_w = self.list.get_focus()
            if focus_w[0] is None:
                continue
            else:
                match focus_w[0].get_widget():
                    case "NodeView":
                        self.pile.contents.append((
                            urwid.Text(f"Node selected"),
                            self.pile.options()))
                    case "ConnectView":
                        name = focus_w[0].get_name()
                        
                        if name in self.model.info.event.keys():
                            values = self.model.info.event.get(name)

                            self.pile.contents.append((
                                urwid.Text(f" {values}"),
                                self.pile.options()))
                    case "SlotView":
                        addr = focus_w[0].get_addr()

                        if addr in self.model.info.msgs.keys():
                            values = self.model.info.msgs.get(addr)

                            for value in values:
                                time = value[0]
                                event = value[1]
                                msg = value[2]

                                self.pile.contents.append((urwid.Text(
                                        f"{time}: {event}: {msg}"),
                                        self.pile.options()))

