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


class Node(DnetWidget):
    def set_txt(self, is_empty: bool):
        if is_empty:
            txt = urwid.Text(f"{self.node_name} (offline)")
            super().update(txt)
        else:
            txt = urwid.Text(f"{self.node_name}")
            super().update(txt)


class Session(DnetWidget):
    def set_txt(self):
        txt = urwid.Text(f"  {self.session}")
        super().update(txt)


class Slot(DnetWidget):
    def set_txt(self, i, addr):
        self.i = i
        if len(self.i) == 1:
            self.addr = addr[0]
            self.id = addr[1]
            txt = urwid.Text(f"    {self.i}: {self.addr}")
        else:
            self.addr = addr
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
       node.set_txt(False)
       self.listwalker.contents.append(node)

       if 'outbound' in info and info['outbound']:
           session = Session(node_name, "outbound")
           session.set_txt()
           self.listwalker.contents.append(session)
           for i, addr in info['outbound'].items():
               slot = Slot(node_name, "outbound-slot")
               slot.set_txt(i, addr)
               self.listwalker.contents.append(slot)

       if 'inbound' in info and info['inbound']:
           if any(info['inbound'].values()):
               session = Session(node_name, "inbound")
               session.set_txt()
               self.listwalker.contents.append(session)
               for i, addr in info['inbound'].items():
                   if bool(addr):
                       slot = Slot(node_name, "inbound-slot")
                       slot.set_txt(i, addr)
                       self.listwalker.contents.append(slot)

       if 'manual' in info and info['manual']:
           session = Session(node_name, "manual")
           session.set_txt()
           self.listwalker.contents.append(session)
           for i, addr in info['manual'].items():
               slot = Slot(node_name, "manual-slot")
               slot.set_txt(i, addr)
               self.listwalker.contents.append(slot)

       if 'seed' in info and info['seed']:
           session = Session(node_name, "seed")
           session.set_txt()
           self.listwalker.contents.append(session)
           for i, info in info['seed'].items():
               slot = Slot(node_name, "seed-slot")
               slot.set_txt(i, addr)
               self.listwalker.contents.append(slot)

    def draw_empty(self, node_name, info):
       node = Node(node_name, "node")
       node.set_txt(True)
       self.listwalker.contents.append(node)

    #-----------------------------------------------------------------
    # Render subscribe_events() (left menu)
    #-----------------------------------------------------------------
    def fill_left_box(self):
        live_inbound = []
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
        live_nodes = []
        dead_nodes = []
        known_nodes = []
        known_inbound = []
        known_outbound = []
        refresh = False

        while True:
            await asyncio.sleep(0.1)

            nodes = self.model.nodes.items()
            listw = self.listwalker.contents
            evloop.call_soon(loop.draw_screen)

            for index, item in enumerate(listw):
                # Keep track of known nodes.
                if item.node_name not in known_nodes:
                    known_nodes.append(item.node_name)
                # Keep track of known inbounds.
                if (item.session == "inbound-slot"
                        and item.i not in known_inbound):
                    known_inbound.append(item.i)
                # Keep track of known outbounds.
                if (item.session == "outbound-slot"
                        and item.id not in known_outbound
                        and not item.id == 0):
                    known_outbound.append(item.id)

            for name, info in nodes:
                # 1. Sort nodes into lists.
                if bool(info) and name not in live_nodes:
                    live_nodes.append(name)
                if not bool(info) and name not in dead_nodes:
                    dead_nodes.append(name)
                if bool(info) and name in dead_nodes:
                    logging.debug("Refresh: dead node online.")
                    refresh = True
                if not bool(info) and name in live_nodes:
                    logging.debug("Refresh: online node offline.")
                    refresh = True

                # 2. Display nodes according to list.
                if name in live_nodes and name not in known_nodes:
                    self.draw_info(name, info)
                if name in dead_nodes and name not in known_nodes:
                    self.draw_empty(name, info)
                if refresh:
                    logging.debug("Refresh initiated.")
                    await asyncio.sleep(0.1)
                    known_outbound.clear()
                    known_inbound.clear()
                    known_nodes.clear()
                    live_nodes.clear()
                    dead_nodes.clear()
                    refresh = False
                    listw.clear()
                    logging.debug("Refresh complete.")

                # 3. Handle events on nodes we know.
                if bool(info) and name in known_nodes:
                    self.fill_left_box()
                    self.fill_right_box()
                    if 'inbound' in info:
                        for key in info['inbound'].keys():
                            # New inbound online.
                            if key not in known_inbound:
                                addr = info['inbound'].get(key)
                                if bool(addr):
                                    logging.debug(f"Refresh: inbound {key} online")
                                    refresh = True
                            # Known inbound offline.
                            for key in known_inbound:
                                addr = info['inbound'].get(key)
                                if bool(addr):
                                    continue
                                logging.debug(f"Refresh: inbound {key} offline")
                                refresh = True
                    # New outbound online.
                    if 'outbound' in info:
                        for i, info in info['outbound'].items():
                            addr = info[0]
                            id = info[1]
                            if id == 0:
                                continue
                            if id in known_outbound:
                                continue
                            logging.debug(f"Outbound {key} came online.")
                            refresh = True
