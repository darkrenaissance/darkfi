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

# TODO: re-implement offline nodes comes online, online node goes offline
# handling.
# TODO: update to latest urwid version.
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


class Node(DnetWidget):
    def set_txt(self, is_empty: bool):
        if is_empty:
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
              ('body','light gray','default', 'standout'),
              ('line','dark cyan','default','standout'),
              ]

    def __init__(self, model):
        self.model = model
        self.pile = urwid.Pile([urwid.Text("")])
        scroll = ScrollBar(Scrollable(self.pile))
        rightbox = urwid.LineBox(scroll)
        self.listwalker = urwid.SimpleListWalker([])
        self.list = urwid.ListBox(self.listwalker)
        leftbox = urwid.LineBox(self.list)
        columns = urwid.Columns([leftbox, rightbox], focus_column=0)
        self.ui = urwid.Frame(urwid.AttrWrap( columns, 'body' ))
        self.active_sessions = set()
        self.active_nodes = set()
        self.refresh_needed = False

    def add_node(self, name, info):
        logging.debug("add_node() [START]")
        node = Node(name, "node")
        if not info:
            node.set_txt(True)
        else:
            node.set_txt(False)
        self.listwalker.append(node)
        self.active_nodes.add(name)

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
                        self.active_sessions.add(addr[1])
                case "inbound" | "manual " | "seed":
                    self.active_sessions.add(i)

    def add_lilith(self, name, info):
        node = Node(name, "lilith-node")
        node.set_txt()
        self.listwalker.append(node)
        self.active_nodes.add(name)
        for (i, key) in enumerate(info['spawns'].keys()):
            slot = Slot(name, "spawn-slot")
            slot.set_txt(i, key)
            self.listwalker.append(slot)

    def update_node(self, name, info):
        for index, widget in enumerate(self.listwalker):
            if isinstance(widget, Node) and widget.name == name:
                if not info:
                    widget.set_txt(True)
                else:
                    widget.set_txt(False)
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
                widget.set_txt(i, addr)
                break

    #-----------------------------------------------------------------
    # Render dnet.subscribe_events() RPC call 
    # Left hand panel only
    #-----------------------------------------------------------------
    def update_left_box(self):
        for index, item in enumerate(self.listwalker):
            # Update outbound slot info
            if item.kind == "outbound-slot":
                key = (f"{item.name}", f"{item.i}")
                if key in self.model.nodes[item.name]['event']:
                    info = self.model.nodes[item.name]['event'].get(key)
                    slot = Slot(item.name, item.kind)
                    slot.set_txt(item.i, info)
                    self.listwalker[index] = slot

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
                if key in info['event']:
                    ev = info['event'].get(key)
                    self.pile.contents.append((
                        urwid.Text(f" {ev}"),
                        self.pile.options()))
            case "outbound-slot" | "inbound-slot" | \
                    "manual-slot" | "seed-slot":
                addr = focus_w[0].addr
                name = focus_w[0].name
                info = self.model.nodes.get(name)

                if addr in info['msgs']:
                    msg = info['msgs'].get(addr)
                    for m in msg:
                        time = m[0]
                        event = m[1]
                        msg = m[2]
                        self.pile.contents.append((urwid.Text(
                                f"{time}: {event}: {msg}"),
                                self.pile.options()))
            case "spawn-slot":
                if session == "spawn-slot":
                    name = focus_w[0].name
                    spawn_name = focus_w[0].id
                    lilith = self.model.liliths.get(name)
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

    def refresh(self):
        logging.debug("Refresh initiated.")
        self.listwalker.clear()
        self.active_nodes.clear()
        self.active_sessions.clear()

        # Repopulate
        for name, info in self.model.nodes.items():
            logging.debug(f"refresh(): {name}, {info}")
            self.add_node(name, info)
        for name, info in self.model.liliths.items():
            self.add_lilith(name, info)

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
                    if info is None:
                        continue

                    # Check for new nodes or update existing slots.
                    if name not in self.active_nodes:
                        self.add_node(name, info)
                    else:
                        start_index = self.update_node(name, info)
                        if start_index is not None:
                            self.update_slots(name, info)

                    for name, info in self.model.liliths.items():
                        if name not in self.active_nodes:
                            self.add_lilith(name, info)

                    # Check for outbound or inbound connections coming
                    # online or going offline, which requires a redraw.
                    if 'outbound' in info:
                        for i, (addr, id) in info['outbound'].items():
                            if id > 0 and id not in self.active_sessions:
                                logging.debug(f"Outbound {id}, {addr} came online.")
                                self.refresh_needed = True
                                break

                    if 'inbound' in info:
                        for key, addr in info['inbound'].items():
                            if key in self.active_sessions and not addr:
                                logging.debug(f"Inbound {key} went offline.")
                                # Delete this key from the model.
                                del(info['inbound'][f'{key}'])
                                self.refresh_needed = True     
                                break
                            if key not in self.active_sessions:
                                logging.debug(f"Inbound {key}, {addr} came online.")
                                self.refresh_needed = True
                                break

            self.update_left_box()
            self.update_right_box()

            evloop.call_soon(loop.draw_screen)
