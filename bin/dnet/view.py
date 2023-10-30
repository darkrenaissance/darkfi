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


class DnetWidget(urwid.WidgetWrap):
    def __init__(self, node_name, session):
        self.node_name = node_name
        self.session = session

    def selectable(self):
        return True

    def keypress(self, size, key):
        return key

    def update(self, txt):
        super().__init__(txt)
        self._w = urwid.AttrWrap(self._w, None)
        self._w.focus_attr = 'line'

    def is_empty(self):
        self.is_empty == True


class Node(DnetWidget):
    def set_txt(self):
        txt = urwid.Text(f"{self.node_name}")
        super().update(txt)


class Session(DnetWidget):
    def set_txt(self):
        txt = urwid.Text(f"  {self.session}")
        super().update(txt)


class Slot(DnetWidget):
    def set_txt(self, i, addr):
        self.i = i
        self.addr = addr
        if len(self.i) == 1:
            txt = urwid.Text(f"    {self.i}: {self.addr}")
        else:
            txt = urwid.Text(f"    {self.addr}")
        super().update(txt)


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

    #-----------------------------------------------------------------
    # Render get_info()
    #-----------------------------------------------------------------
    def draw_info(self, node_name, info):
       node = Node(node_name, "node")
       node.set_txt()
       self.listwalker.contents.append(node)

       if info['outbound']:
           session = Session(node_name, "outbound")
           session.set_txt()
           self.listwalker.contents.append(session)
           for i, addr in info['outbound'].items():
               slot = Slot(node_name, "outbound-slot")
               slot.set_txt(i, addr)
               self.listwalker.contents.append(slot)

       if info['inbound']:
           session = Session(node_name, "inbound")
           session.set_txt()
           self.listwalker.contents.append(session)
           for i, addr in info['inbound'].items():
               slot = Slot(node_name, "inbound-slot")
               slot.set_txt(i, addr)
               self.listwalker.contents.append(slot)

       if info['manual']:
           session = Session(node_name, "manual")
           session.set_txt()
           self.listwalker.contents.append(session)
           for i, addr in info['manual'].items():
               slot = Slot(node_name, "manual-slot")
               slot.set_txt(i, addr)
               self.listwalker.contents.append(slot)

       if info['seed']:
           session = Session(node_name, "seed")
           session.set_txt()
           self.listwalker.contents.append(session)
           for i, info in info['seed'].items():
               slot = Slot(node_name, "seed-slot")
               slot.set_txt(i, addr)
               self.listwalker.contents.append(slot)

    def draw_empty(self, node_name, info):
       name = node_name + " (offline)" 
       node = Node(name, "node")
       node.set_txt()
       self.listwalker.contents.append(node)

    #-----------------------------------------------------------------
    # Render subscribe_events() (left menu)
    #-----------------------------------------------------------------
    def fill_left_box(self):
        known_inbound = []
        new_inbound= {}
        for index, item in enumerate(self.listwalker.contents):
            # Update outbound slot info
            if item.session == "outbound-slot":
                key = (f"{item.node_name}", f"{item.i}")
                if key in self.model.nodes[item.node_name]['event']:
                    info = self.model.nodes[item.node_name]['event'].get(key)
                    slot = Slot(item.node_name, item.session)
                    slot.set_txt(item.i, info)
                    self.listwalker.contents[index] = slot

    #-----------------------------------------------------------------
    # Render subscribe_events() (right menu)
    #-----------------------------------------------------------------
    def fill_right_box(self):
        self.pile.contents.clear()
        focus_w = self.list.get_focus()
        if focus_w[0] is None:
            return
        session = focus_w[0].session

        if session == "outbound":
            key = (focus_w[0].node_name, "outbound")
            info = self.model.nodes.get(focus_w[0].node_name)
            if key in info['event']:
                ev = info['event'].get(key)
                self.pile.contents.append((
                    urwid.Text(f" {ev}"),
                    self.pile.options()))

        if (session == "outbound-slot" or session == "inbound-slot"
                or session == "manual-slot" or session == "seed-slot"):
            addr = focus_w[0].addr
            node_name = focus_w[0].node_name
            info = self.model.nodes.get(node_name)
            if addr in info['msgs']:
                msg = info['msgs'].get(addr)
                for m in msg:
                    time = m[0]
                    event = m[1]
                    msg = m[2]
                    self.pile.contents.append((urwid.Text(
                            f"{time}: {event}: {msg}"),
                            self.pile.options()))

    async def update_view(self, evloop: asyncio.AbstractEventLoop,
                          loop: urwid.MainLoop):
        known_nodes = []
        empty_nodes = []
        while True:
            await asyncio.sleep(0.1)
            # Redraw the screen
            evloop.call_soon(loop.draw_screen)

            for index, item in enumerate(self.listwalker.contents):
                known_nodes.append(item.node_name)

            # Draw get_info() -> called once
            for node_name, info in self.model.nodes.items():
                if node_name in known_nodes:
                    continue
                else:
                    self.draw_info(node_name, info)
            # TODO:
            # There are a few events that should trigger a redraw:
            #   * a new inbound connection comes online
            #   * a inbound connection has gone offline
            #   * a new node comes online (FIXME)
            #   * when RPC can't connect, display the node as offline.

            # Check for offline nodes
            for node_name, info in self.model.nodes.items():
                if not bool(info):
                    if node_name in empty_nodes:
                        continue
                    else:
                        empty_nodes.append(node_name)
                        self.listwalker.contents.clear()
                        self.draw_empty(node_name, info)
                        for name, info in self.model.nodes.items():
                            if name not in empty_nodes:
                                self.draw_info(name, info)
                # Only render info if the node is online
                self.fill_left_box()
                self.fill_right_box()
