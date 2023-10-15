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

    def __init__(self, info):
        self.name = info
        self.text = urwid.Text(f"{self.name}")
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

    def __init__(self, info):
        self.name = info
        self.text = urwid.Text(f"{self.name}")
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


class View():
    palette = [
              ('body','light gray','black', 'standout'),
              ("line","dark cyan","black","standout"),
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
        names = []
        while True:
            await asyncio.sleep(0.1)
            for item in self.listwalker.contents:
                name = item.get_name()
                names.append(name)

            for name, values in self.model.nodes.items():
                # Update events
                if name in names:
                    for key, value in values.outbounds.items():
                        if len(value) == 1:
                            continue
                        else:
                            slot = SlotView(f"    {key}: {str(value[1])}")
                            self.listwalker.contents[int(key)] = widget
                # Update get_info()
                else:
                    widget = NodeView(name)
                    self.listwalker.contents.append(widget)

                    outbounds = values.outbounds
                    logging.debug("outbounds", outbounds)
                    inbound = values.inbound
                    manual = values.manual
                    seed = values.seed

                    if len(outbounds) != 0:
                        widget = ConnectView("  outbound")
                        self.listwalker.contents.append(widget)
                        for num, info in outbounds.items():
                            widget = SlotView(f"    {num}: {info[0]}")
                            self.listwalker.contents.append(widget)

                    if len(inbound) != 0:
                        widget = ConnectView("  inbound")
                        self.listwalker.contents.append(widget)

                    if len(seed) != 0:
                        widget = ConnectView("  seed")
                        self.listwalker.contents.append(widget)

                    if len(manual) != 0:
                        widget = ConnectView("  manual")
                        self.listwalker.contents.append(widget)

    async def render_info(self):
        while True:
            await asyncio.sleep(0.1)
            self.pile.contents.clear()
            focus_w = self.list.get_focus()
            match focus_w[0].get_widget():

                case "NodeView":
                    self.pile.contents.append((
                        urwid.Text(f"Node selected"),
                        self.pile.options()))

                case "ConnectView":
                    self.pile.contents.append((
                        urwid.Text("Connection selected"),
                        self.pile.options()))

                case "SlotView":
                    numbered_name = focus_w[0].get_name()
                    # Remove numbering
                    name = numbered_name[7:]

                    if name in self.model.info.msgs.keys():
                        values = (self.model.info.msgs.get(name))

                        for value in values:
                            time = value[0]
                            event = value[1]
                            msg = value[2]

                            self.pile.contents.append((urwid.Text(
                                    f"{time}: {event}: {msg}"),
                                    self.pile.options()))

