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


class NodeView(urwid.WidgetWrap):

    def __init__(self, info):
        self.type = "node"
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

    def get_type(self):
        return self.type

class ConnectView(urwid.WidgetWrap):

    def __init__(self, node, kind):
        self.type = f"{kind}-connect"
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

    def get_type(self):
        return self.type

class SlotView(urwid.WidgetWrap):

    def __init__(self, node, type, id, info):
        self.id = id
        self.type = type
        self.name = (f"{node}", f"{id}")
        self.addr = info
        if len(id) == 1:
            self.text = urwid.Text(f"    {id}: {self.addr}")
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

    def get_type(self):
        return self.type


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
        self.list = urwid.ListBox(self.listwalker)
        leftbox = urwid.LineBox(self.list)
        columns = urwid.Columns([leftbox, rightbox], focus_column=0)
        self.ui = urwid.Frame(urwid.AttrWrap( columns, 'body' ))


    async def update_view(self, evloop: asyncio.AbstractEventLoop,
                          loop: urwid.MainLoop):
        known_nodes = []
        known_inbound = []
        while True:
            await asyncio.sleep(0.1)
            # Redraw the screen
            evloop.call_soon(loop.draw_screen)

            for index, item in enumerate(self.listwalker.contents):
                known_nodes.append(item.get_name())

            # Render get_info()
            for node, values in self.model.nodes.items():
                if node in known_nodes:
                    continue
                else:
                    widget = NodeView(node)
                    self.listwalker.contents.append(widget)

                    if values['outbound']:
                        widget = ConnectView(node, "outbound")
                        self.listwalker.contents.append(widget)
                        for i, info in values['outbound'].items():
                            widget = SlotView(node, "outbound", i, info)
                            self.listwalker.contents.append(widget)

                    if values['inbound']:
                        widget = ConnectView(node, "inbound")
                        self.listwalker.contents.append(widget)
                        for i, info in values['inbound'].items():
                            widget = SlotView(node, "inbound", i, info)
                            self.listwalker.contents.append(widget)

                    if values['manual']:
                        widget = ConnectView(node, "manual")
                        self.listwalker.contents.append(widget)
                        for i, info in values['manual'].items():
                            widget = SlotView(node, "manual", i, info)
                            self.listwalker.contents.append(widget)

                    if values['seed']:
                        widget = ConnectView(node, "seed")
                        self.listwalker.contents.append(widget)
                        for i, info in values['seed'].items():
                            widget = SlotView(node, "seed", i, info)
                            self.listwalker.contents.append(widget)


            # Update outbound slot info
            for index, item in enumerate(self.listwalker.contents):
                if item.get_type() == "outbound":
                    name = item.get_name()
                    node = name[0]
                    if name in self.model.nodes[node]['event']:
                        value = self.model.nodes[node]['event'].get(name)
                        widget = SlotView(node, "outbound", name[1], value)
                        self.listwalker.contents[index] = widget

            # Update new inbound connections
            for index, item in enumerate(self.listwalker.contents):
                if item.get_type() == "inbound":
                    name = item.get_name()
                    if name[1] not in known_inbound:
                        known_inbound.append(name[1])
            for node, value in self.model.nodes.items():
                for id, addr in value['inbound'].items():
                   if id in known_inbound:
                       continue
                   else:
                       widget = SlotView(node, "inbound", id, addr)
                       self.listwalker.contents.append(widget)

            # Remove disconnected inbounds
            for inbound in known_inbound:
                for value in self.model.nodes.values():
                    if inbound in value['inbound']:
                        continue
                    for index, item in enumerate(self.listwalker.contents):
                        name = item.get_name()
                        if name[1] == id:
                            del self.listwalker.contents[index]
            

    # Render subscribe_events() (right menu)
    async def render_info(self, evloop: asyncio.AbstractEventLoop,
                          loop: urwid.MainLoop):
        while True:
            await asyncio.sleep(0.01)
            # Redraw the screen
            evloop.call_soon(loop.draw_screen)

            self.pile.contents.clear()
            focus_w = self.list.get_focus()
            if focus_w[0] is None:
                continue
            else:
                match focus_w[0].get_widget():
                    case "NodeView":
                        # TODO: We will display additional node info here.
                        self.pile.contents.append((
                            urwid.Text(f""),
                            self.pile.options()))
                    case "ConnectView":
                        name = focus_w[0].get_name()
                        info = self.model.nodes.get(name[0])
                        if name in info['event']:
                            ev = info['event'].get(name)

                            self.pile.contents.append((
                                urwid.Text(f" {ev}"),
                                self.pile.options()))
                    case "SlotView":
                        addr = focus_w[0].get_addr()
                        name = focus_w[0].get_name()
                        info = self.model.nodes.get(name[0])
                        if addr in info['msgs']:
                            msg = info['msgs'].get(addr)

                            for m in msg:
                                time = m[0]
                                event = m[1]
                                msg = m[2]

                                self.pile.contents.append((urwid.Text(
                                        f"{time}: {event}: {msg}"),
                                        self.pile.options()))

