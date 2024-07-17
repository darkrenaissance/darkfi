# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2024 Dyne.org foundation
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

from src.scroll import ScrollBar, Scrollable
from src.model import Model


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
        if self.session == "outbound-slot":
            self.addr = addr[0]
            self.id = addr[1]
            txt = urwid.Text(f"    {self.i}: {self.addr}")

        if self.session == "spawn-slot":
            self.id = addr
            txt = urwid.Text(f"    {addr}")

        if (self.session == "manual-slot"
            or self.session == "seed-slot"
            or self.session == "inbound-slot"):
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
        self.listw = self.listwalker.contents
        self.list = urwid.ListBox(self.listwalker)
        leftbox = urwid.LineBox(self.list)
        columns = urwid.Columns([leftbox, rightbox], focus_column=0)
        self.ui = urwid.Frame(urwid.AttrWrap( columns, 'body' ))
        self.known_outbound = []
        self.known_inbound = []
        self.known_nodes = []
        self.live_nodes = []
        self.dead_nodes = []
        self.refresh = False

    #-----------------------------------------------------------------
    # Render dnet.get_info() RPC call
    #-----------------------------------------------------------------
    def draw_info(self, node_name, info):
        #logging.debug('draw_info() [START]')
        if 'spawns' in info:
            #logging.debug(f'drawing lilith name={node_name} info={info}')
            self.draw_lilith(node_name, info)

        else:
            #logging.debug(f'drawing node name={node_name} info={info}')
            node = Node(node_name, "node")
            node.set_txt(False)
            self.listw.append(node)
            
            if 'outbound' in info and info['outbound']:
                session = Session(node_name, "outbound")
                session.set_txt()
                self.listw.append(session)
                for i, addr in info['outbound'].items():
                    slot = Slot(node_name, "outbound-slot")
                    slot.set_txt(i, addr)
                    self.listw.append(slot)

            if 'inbound' in info and info['inbound']:
                if any(info['inbound'].values()):
                    session = Session(node_name, "inbound")
                    session.set_txt()
                    self.listw.append(session)
                    for i, addr in info['inbound'].items():
                        if bool(addr):
                            slot = Slot(node_name, "inbound-slot")
                            slot.set_txt(i, addr)
                            self.listw.append(slot)

            if 'manual' in info and info['manual']:
                session = Session(node_name, "manual")
                session.set_txt()
                self.listw.append(session)
                for i, addr in info['manual'].items():
                    slot = Slot(node_name, "manual-slot")
                    slot.set_txt(i, addr)
                    self.listw.append(slot)

            if 'seed' in info and info['seed']:
                session = Session(node_name, "seed")
                session.set_txt()
                self.listw.append(session)
                for i, info in info['seed'].items():
                    slot = Slot(node_name, "seed-slot")
                    slot.set_txt(i, addr)
                    self.listw.append(slot)

    def draw_lilith(self, node_name, info):
        node = Node(node_name, "lilith-node")
        node.set_txt(False)
        self.listw.append(node)
        for (i, key) in enumerate(info['spawns'].keys()):
            slot = Slot(node_name, "spawn-slot")
            slot.set_txt(i, key)
            self.listw.append(slot)

    def draw_empty(self, node_name, info):
        node = Node(node_name, "node")
        node.set_txt(True)
        self.listw.append(node)

    #-----------------------------------------------------------------
    # Render dnet.subscribe_events() RPC call 
    # Left hand panel only
    #-----------------------------------------------------------------
    def fill_left_box(self):
        live_inbound = []
        new_inbound= {}
        for index, item in enumerate(self.listw):
            # Update outbound slot info
            if item.session == "outbound-slot":
                key = (f"{item.node_name}", f"{item.i}")
                if key in self.model.nodes[item.node_name]['event']:
                    info = self.model.nodes[item.node_name]['event'].get(key)
                    slot = Slot(item.node_name, item.session)
                    slot.set_txt(item.i, info)
                    self.listw[index] = slot

    #-----------------------------------------------------------------
    # Render lilith.spawns() RPC call 
    # Right hand panel only
    #-----------------------------------------------------------------
    def fill_lilith_right_box(self):
        self.pile.contents.clear()
        focus_w = self.list.get_focus()
        if focus_w[0] is None:
            return
        session = focus_w[0].session
        if session == "spawn-slot":
            node_name = focus_w[0].node_name
            spawn_name = focus_w[0].id
            lilith = self.model.liliths.get(node_name)
            spawns = lilith.get('spawns')
            info = spawns.get(spawn_name)

            if info['urls']:
                urls = info['urls']
                self.pile.contents.append((urwid.Text(
                    f"Accept addrs:"),
                    self.pile.options()))
                for url in urls:
                    self.pile.contents.append((urwid.Text(
                        f"  {url}"),
                        self.pile.options()))

            if info['whitelist']:
                whitelist = info['whitelist']
                self.pile.contents.append((urwid.Text(
                    f"Whitelist:"),
                    self.pile.options()))
                for host in whitelist:
                    self.pile.contents.append((urwid.Text(
                        f"  {host}"),
                        self.pile.options()))

            if info['greylist']:
                greylist = info['greylist']
                self.pile.contents.append((urwid.Text(
                    f"Greylist:"),
                    self.pile.options()))
                for host in greylist:
                    self.pile.contents.append((urwid.Text(
                        f"  {host}"),
                        self.pile.options()))

            if info['goldlist']:
                goldlist = info['goldlist']
                self.pile.contents.append((urwid.Text(
                    f"Goldlist:"),
                    self.pile.options()))
                for host in goldlist:
                    self.pile.contents.append((urwid.Text(
                        f"  {host}"),
                        self.pile.options()))

    #-----------------------------------------------------------------
    # Render dnet.subscribe_events() RPC call
    # Right hand menu only
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

        if session == "spawn-slot":
            node_name = focus_w[0].node_name
            spawn_name = focus_w[0].id
            lilith = self.model.liliths.get(node_name)
            spawns = lilith.get('spawns')
            info = spawns.get(spawn_name)

            if info['urls']:
                urls = info['urls']
                self.pile.contents.append((urwid.Text(
                    f"Accept addrs:"),
                    self.pile.options()))
                for url in urls:
                    self.pile.contents.append((urwid.Text(
                        f"  {url}"),
                        self.pile.options()))

            if info['whitelist']:
                whitelist = info['whitelist']
                self.pile.contents.append((urwid.Text(
                    f"Whitelist:"),
                    self.pile.options()))
                for host in whitelist:
                    self.pile.contents.append((urwid.Text(
                        f"  {host}"),
                        self.pile.options()))

            if info['greylist']:
                greylist = info['greylist']
                self.pile.contents.append((urwid.Text(
                    f"Greylist:"),
                    self.pile.options()))
                for host in greylist:
                    self.pile.contents.append((urwid.Text(
                        f"  {host}"),
                        self.pile.options()))

            if info['goldlist']:
                goldlist = info['goldlist']
                self.pile.contents.append((urwid.Text(
                    f"Goldlist:"),
                    self.pile.options()))
                for host in goldlist:
                    self.pile.contents.append((urwid.Text(
                        f"  {host}"),
                        self.pile.options()))

    #-----------------------------------------------------------------
    # Sort through node info, checking whether we are already 
    # tracking this node or if the node's state has changed.
    #-----------------------------------------------------------------
    def sort(self, nodes):
        for name, info in nodes:
            if bool(info) and name not in self.live_nodes:
                self.live_nodes.append(name)
            if not bool(info) and name not in self.dead_nodes:
                self.dead_nodes.append(name)
            if bool(info) and name in self.dead_nodes:
                logging.debug("Refresh: dead node online.")
                self.refresh = True
            if not bool(info) and name in self.live_nodes:
                logging.debug("Refresh: online node offline.")
                self.refresh = True

    #-----------------------------------------------------------------
    # Checks whether we are already displaying this node, and draw
    # it if not. 
    #-----------------------------------------------------------------
    async def display(self, nodes):
        for name, info in nodes:
            if name in self.live_nodes and name not in self.known_nodes:
                self.draw_info(name, info)
            if name in self.dead_nodes and name not in self.known_nodes:
                self.draw_empty(name, info)
            if self.refresh:
                logging.debug("Refresh initiated.")
                await asyncio.sleep(0.1)
                self.known_outbound.clear()
                self.known_inbound.clear()
                self.known_nodes.clear()
                self.live_nodes.clear()
                self.dead_nodes.clear()
                self.refresh = False
                self.listw.clear()
                logging.debug("Refresh complete.")
    #-----------------------------------------------------------------
    # Handle events.
    #-----------------------------------------------------------------
    def draw_events(self, nodes):
        for name, info in nodes:
            if bool(info) and name in self.known_nodes:
                self.fill_left_box()
                self.fill_right_box()

                if 'inbound' in info:
                    # New inbound online.
                    for key in info['inbound'].keys():
                        if key not in self.known_inbound:
                            addr = info['inbound'].get(key)
                            if not bool(addr) or not addr == None:
                                continue
                            logging.debug(f"Refresh: inbound {key} online")
                            self.refresh = True

                    # Known inbound offline.
                    for key in self.known_inbound:
                        addr = info['inbound'].get(key)
                        if bool(addr) or addr == None:
                            continue
                        logging.debug(f"Refresh: inbound {key} offline")
                        self.refresh = True

                # New outbound online.
                if 'outbound' in info:
                    for i, info in info['outbound'].items():
                        addr = info[0]
                        id = info[1]
                        if id == 0:
                            continue
                        if id in self.known_outbound:
                            continue
                        logging.debug(f"Outbound {i}, {addr} came online.")
                        self.refresh = True
    
    async def update_view(self, evloop: asyncio.AbstractEventLoop,
                          loop: urwid.MainLoop):
        while True:
            await asyncio.sleep(0.1)

            nodes = self.model.nodes.items()
            liliths = self.model.liliths.items()
            evloop.call_soon(loop.draw_screen)

            # We first ensure that we are keeping track
            # of all the displayed widgets.
            for index, item in enumerate(self.listw):
                # Keep track of known nodes.
                if item.node_name not in self.known_nodes:
                    self.known_nodes.append(item.node_name)
                # Keep track of known inbounds.
                if (item.session == "inbound-slot"
                        and item.i not in self.known_inbound):
                    self.known_inbound.append(item.i)
                # Keep track of known outbounds.
                if (item.session == "outbound-slot"
                        and item.id not in self.known_outbound
                        and not item.id == 0):
                    self.known_outbound.append(item.id)

            self.sort(nodes)
            self.sort(liliths)
            
            await self.display(nodes)
            await self.display(liliths)

            self.fill_lilith_right_box()
            self.draw_events(nodes)
