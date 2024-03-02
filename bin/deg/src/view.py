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

from src.scroll import ScrollBar, Scrollable

class DegWidget(urwid.WidgetWrap):
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


class Node(DegWidget):
    def set_txt(self, is_empty: bool):
        if is_empty:
            txt = urwid.Text(f"{self.node_name} (offline)")
            super().update(txt)
        else:
            txt = urwid.Text(f"{self.node_name}")
            super().update(txt)


class Session(DegWidget):
    def set_txt(self):
        txt = urwid.Text(f"  {self.session}")
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
        self.known_nodes = []
        self.live_nodes = []
        self.dead_nodes = []
        self.refresh = False

    #-----------------------------------------------------------------
    # Render deg.get_info() RPC call
    #-----------------------------------------------------------------
    def draw_info(self, node_name, info):
        logging.debug(f'info {info}')
        # info = info['result']
        # if 'eventgraph_info' in info:
            # logging.debug(f'info {info}')
        node = Node(node_name, "node")
        node.set_txt(False)
        self.listw.append(node)

        # info = info['eventgraph_info']
        
        if 'current_genesis' in info and info['current_genesis']:
            session = Session(node_name, "current_genesis")
            session.set_txt()
            self.listw.append(session)
        
        if 'broadcasted_ids' in info and info['broadcasted_ids']:
            session = Session(node_name, "broadcasted_ids")
            session.set_txt()
            self.listw.append(session)

        if 'synced' in info and info['synced']:
            session = Session(node_name, "synced")
            session.set_txt()
            self.listw.append(session)

        if 'unreferenced_tips' in info and info['unreferenced_tips']:
            session = Session(node_name, "unreferenced_tips")
            session.set_txt()
            self.listw.append(session)


    def draw_empty(self, node_name, info):
        node = Node(node_name, "node")
        node.set_txt(True)
        self.listw.append(node)

    #-----------------------------------------------------------------
    # Render deg.subscribe_events() RPC call
    # Right hand menu only
    #-----------------------------------------------------------------
    def fill_right_box(self):
        self.pile.contents.clear()
        focus_w = self.list.get_focus()
        if focus_w[0] is None:
            return
        session = focus_w[0].session

        if session == "node":
            node_name = focus_w[0].node_name
            info = self.model.nodes.get(node_name)
            if info['msgs']:
                msg = info['msgs'].get(node_name)
                for m in msg:
                    time = m[0]
                    event = m[1]
                    event_info = m[2]
                    msg = m[3]
                    self.pile.contents.append((urwid.Text(
                            f"{time}: {event} {event_info}: {msg}"),
                            self.pile.options()))

        if session == "current_genesis":
            key = "current_genesis"
            node_name = focus_w[0].node_name
            info = self.model.nodes.get(node_name)

            if key in info:
                ev = info.get(key)
                self.pile.contents.append((
                    urwid.Text(f" {ev}"),
                    self.pile.options()))
        
        if session == "broadcasted_ids":
            key = "broadcasted_ids"
            node_name = focus_w[0].node_name
            info = self.model.nodes.get(node_name)

            if key in info:
                ev = list(info.get(key))
                if info['msgs']:
                    msg = info['msgs'].get(node_name)
                    for m in msg:
                        event = m[1]
                        event_info = m[2]
                        msg = m[3]
                        if event_info == "EventPut" and event == "send" and msg not in ev:
                            ev.extend(msg)
                self.pile.contents.append((
                    urwid.Text(f" {ev}"),
                    self.pile.options()))

        if session == "synced":
            key = "synced"
            node_name = focus_w[0].node_name
            info = self.model.nodes.get(node_name)

            if key in info:
                ev = info.get(key)
                self.pile.contents.append((
                    urwid.Text(f" {ev}"),
                    self.pile.options()))
        
        if session == "unreferenced_tips":
            key = "unreferenced_tips"
            node_name = focus_w[0].node_name
            info = self.model.nodes.get(node_name)

            if key in info:
                ev = list(info.get(key))
                if info['msgs']:
                    msg = info['msgs'].get(node_name)
                    for m in msg:
                        event = m[1]
                        event_info = m[2]
                        msg = m[3]
                        if event_info == "dag_insert" and event == "send":
                            ev = msg
                self.pile.contents.append((
                    urwid.Text(f" {ev}"),
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
                self.fill_right_box()
                
                if 'unreferenced_tips' in info:
                        val = info['unreferenced_tips']
                        if not bool(val) or not val == None:
                            continue
                        logging.debug(f"Refresh: unreferenced_tips")
                        self.refresh = True

    async def update_view(self, evloop: asyncio.AbstractEventLoop,
                          loop: urwid.MainLoop):
        while True:
            await asyncio.sleep(0.1)

            nodes = self.model.nodes.items()
            evloop.call_soon(loop.draw_screen)

            # We first ensure that we are keeping track
            # of all the displayed widgets.
            for index, item in enumerate(self.listw):
                # Keep track of known nodes.
                if item.node_name not in self.known_nodes:
                    self.known_nodes.append(item.node_name)

            self.sort(nodes)
            
            await self.display(nodes)

            self.draw_events(nodes)
