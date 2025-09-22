# This file is part of DarkFi (https://dark.fi)
#
# Copyright (C) 2020-2025 Dyne.org foundation
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
from enum import Enum

from src.model import Model

class DnetWidget(urwid.WidgetWrap):
    def __init__(self, name, kind):
        self.name = name
        self.kind = kind

    def selectable(self):
        return True

    def keypress(self, size, key):
        return key

    def update(self, txt):
        super().__init__(txt)
        self._w = urwid.AttrWrap(self._w, None)
        self._w.focus_attr = 'line'

class NodeState(Enum):
    ON = 0
    OFF = 1

class Node(DnetWidget):
    def __init__(self, name, kind, state):
        self.state = state
        super().__init__(name, kind)

    def set_txt(self):
        if self.state == NodeState.OFF:
            txt = urwid.Text(f"{self.name} (offline)")
            super().update(txt)
        else:
            txt = urwid.Text(f"{self.name}")
            super().update(txt)

class Session(DnetWidget):
    def set_txt(self):
        txt = urwid.Text(f"  {self.kind}")
        super().update(txt)

class Slot(DnetWidget):
    def set_txt(self, i, addr):
        self.i = i
        match self.kind:
            case "outbound-slot":
                self.addr = addr[0]
                self.id = addr[1]
                txt = urwid.Text(f"    {self.i}: {self.addr}")
                super().update(txt)
            case "spawn-slot":
                self.id = addr
                txt = urwid.Text(f"    {addr}")
                super().update(txt)
            case "manual-slot" | "seed-slot" | "inbound-slot":
                self.addr = addr
                txt = urwid.Text(f"    {self.addr}")
                super().update(txt)
    

class View():
    palette = [
              ('body','default','default','standout'),
              ('line','dark cyan','black','standout'),
              ]

    def __init__(self, model):
        self.model = model
        self.pile = urwid.Pile([urwid.Text("")])
        scroll = urwid.ScrollBar(urwid.Scrollable(self.pile))
        rightbox = urwid.LineBox(scroll)
        leftbox = urwid.LineBox(scroll)
        self.listwalker = urwid.SimpleListWalker([])
        self.list = urwid.ListBox(self.listwalker)
        leftbox = urwid.LineBox(self.list)
        columns = urwid.Columns([leftbox, rightbox], focus_column=0)
        self.ui = urwid.Frame(urwid.AttrWrap(columns, 'body'))
        self.sessions = set()
        self.nodes = set()
        self.refresh_needed = False

    def add_node(self, name, info, state):
        logging.debug(f"Adding node: {name} {info} {state}")
        node = Node(name, "node", state)
        node.set_txt()
        self.nodes.add(name)
        self.listwalker.append(node)
        self.add_sessions(name, info)

    def add_sessions(self, name, info):
        for session in ['outbound', 'inbound', 'manual', 'seed']:
            if session in info and info[session]:
                session_widget = Session(name, session)
                session_widget.set_txt()
                self.listwalker.append(session_widget)
                self.add_slots(name, session, info[session])

    def add_slots(self, name, session, slots):
        for i, addr in slots.items():
            slot = Slot(name, f"{session}-slot")
            slot.set_txt(i, addr)
            self.listwalker.append(slot)
            match session:
                case "outbound":
                    if addr[1] > 0:
                        self.sessions.add(addr[1])
                case "inbound" | "manual " | "seed":
                    self.sessions.add(i)

    def add_lilith(self, name, info, state):
        logging.debug(f"Adding lilith: {name} {info} {state}")
        node = Node(name, "lilith-node", state)
        node.set_txt()
        self.nodes.add(name)
        self.listwalker.append(node)
        if state == NodeState.OFF:
            return
        else:
            for (i, key) in enumerate(info['spawns'].keys()):
                slot = Slot(name, "spawn-slot")
                slot.set_txt(i, key)
                self.listwalker.append(slot)

    def update_lilith(self, name, info):
        for index, widget in enumerate(self.listwalker):
            if isinstance(widget, Node) and widget.name == name:
                # Offline node has come online
                if widget.state == NodeState.OFF and info:
                        self.refresh_needed = True
                # Online node has gone offline
                elif widget.state == NodeState.ON and not info:
                        self.refresh_needed = True

    def update_node(self, name, info):
        for index, widget in enumerate(self.listwalker):
            if isinstance(widget, Node) and widget.name == name:
                # Offline node has come online
                if widget.state == NodeState.OFF and info:
                        self.refresh_needed = True
                # Online node has gone offline
                elif widget.state == NodeState.ON and not info:
                        self.refresh_needed = True
                else:
                    widget.set_txt()
                    return index + 1
        return None

    def update_slots(self, name, info):
        if info is None:
            return
        for session in ['outbound', 'inbound', 'manual', 'seed']:
            if session in info and info[session]:
                for i, addr in info[session].items():
                    self.update_slot(name, session, i, addr)

    def update_slot(self, name, session, i, addr):
        for index, widget in enumerate(self.listwalker):
            if isinstance(widget, Slot) and \
                    widget.name == name and \
                    widget.kind == f"{session}-slot" and \
                    widget.i == i:
                key = (f"{widget.name}", f"{widget.i}")
                if key in self.model.nodes[widget.name]['event']:
                    info = self.model.nodes[widget.name]['event'].get(key)
                    widget.set_txt(i, info)
                    self.listwalker[index] = widget
                    break
    
    #-----------------------------------------------------------------
    # Render dnet.subscribe_events() RPC call
    # Right hand menu only
    #-----------------------------------------------------------------
    def update_right_box(self):
        self.pile.contents.clear()
        focus_w = self.list.get_focus()
        if focus_w[0] is None:
            return
        kind = focus_w[0].kind

        match kind:
            case "outbound":
                key = (focus_w[0].name, "outbound")
                info = self.model.nodes.get(focus_w[0].name)
                if info and key in info['event']:
                    ev = info['event'].get(key)
                    self.pile.contents.append((
                        urwid.Text(f" {ev}"),
                        self.pile.options()))
            case "outbound-slot" | "inbound-slot" | \
                    "manual-slot" | "seed-slot":
                addr = focus_w[0].addr
                name = focus_w[0].name
                info = self.model.nodes.get(name)

                if info and addr in info['msgs']:
                    msg = info['msgs'].get(addr)
                    for m in msg:
                        time = m[0]
                        event = m[1]
                        msg = m[2]
                        self.pile.contents.append((urwid.Text(
                                f"{time}: {event}: {msg}"),
                                self.pile.options()))
            case "spawn-slot":
                name = focus_w[0].name
                spawn_name = focus_w[0].id
                lilith = self.model.liliths.get(name)
                spawns = lilith.get('spawns')
                if spawns is None:
                    return

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

    def update_node_state(self, info):
        if info:
            logging.debug(f"update_node_state(): Returning {NodeState.ON}")
            return NodeState.ON
        else:
            logging.debug(f"update_node_state(): Returning {NodeState.OFF}")
            return NodeState.OFF

    def refresh(self):
        logging.debug("Refresh initiated.")
        self.listwalker.clear()
        self.sessions.clear()
        self.nodes.clear()

        # Repopulate
        for name, info in self.model.nodes.items():
            logging.debug(f"refresh nodes(): (nodes) updating node state")
            state = self.update_node_state(info)
            logging.debug(f"refresh nodes(): {name}, {info}, {state}")
            self.add_node(name, info, state)

        for name, info in self.model.liliths.items():
            logging.debug(f"refresh nodes(): (lilith) updating node state")
            state = self.update_node_state(info)
            logging.debug(f"refresh liliths(): {name}, {info}, {state}")
            self.add_lilith(name, info, state)

        logging.debug("Refresh complete.")

    async def update_view(self, evloop: asyncio.AbstractEventLoop,
                          loop: urwid.MainLoop):
        while True:
            await asyncio.sleep(0.1)

            if self.refresh_needed:
                self.refresh()
                self.refresh_needed = False
            else:
                for name, info in self.model.nodes.items():
                    #logging.debug(f"update_view(): found {name}, {info}")
                    # Check for new nodes or update existing slots.
                    if name not in self.nodes:
                        logging.debug(f"update_view(): (node) updating node state")
                        state = self.update_node_state(info)
                        self.add_node(name, info, state)
                    else:
                        start_index = self.update_node(name, info)
                        if start_index is not None:
                            self.update_slots(name, info)
                    # Check for outbound or inbound connections coming
                    # online or going offline, which requires a redraw.
                    if 'outbound' in info:
                        for i, (addr, id) in info['outbound'].items():
                            if id > 0 and id not in self.sessions:
                                logging.debug(f"Outbound {id}, {addr} came online.")
                                self.refresh_needed = True
                                break

                    if 'inbound' in info:
                        for key, addr in info['inbound'].items():
                            if key in self.sessions and not addr:
                                logging.debug(f"Inbound {key} went offline.")
                                # Delete this key from the model.
                                del(info['inbound'][f'{key}'])
                                self.refresh_needed = True     
                                break
                            if key not in self.sessions:
                                logging.debug(f"Inbound {key}, {addr} came online.")
                                self.refresh_needed = True
                                break

                # Check for new lilith nodes. 
                for name, info in self.model.liliths.items():
                    if name not in self.nodes:
                        logging.debug(f"update_view(): (lilith) updating node state")
                        state = self.update_node_state(info)
                        self.add_lilith(name, info, state)
                    else:
                        self.update_lilith(name, info)


            self.update_right_box()
            evloop.call_soon(loop.draw_screen)
