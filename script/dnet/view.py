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

loop = asyncio.get_event_loop()

class ServiceWidget(urwid.TreeWidget):
    def get_display_text(self):
        return self.get_node().get_value()['name']

class ChildNode(urwid.TreeNode):
    def load_widget(self):
        return ServiceWidget(self)

# Service/ session
class ServiceNode(urwid.ParentNode):
    def load_widget(self):
        return ServiceWidget(self)

    def load_child_keys(self):
        data = self.get_value()
        return range(len(data['session']))

    def load_child_node(self, key):
        childdata = self.get_value()['session'][key]
        childdepth = self.get_depth() + 1
        if 'session' in childdata:
            childclass = ServiceNode
        else:
            childclass = ChildNode
        return childclass(childdata, parent=self, key=key, depth=childdepth)

class Dnetview:
    palette = [
              ('body','light gray','black', 'standout'),
              ("line","dark cyan","black","standout"),
              ]

    def __init__(self, data=None):
        self.topnode = ServiceNode(data)

        self.listbox = urwid.Columns([urwid.TreeListBox(urwid.TreeWalker(self.topnode))])
        self.listbox.offset_rows = 1
        
        list_frame = urwid.LineBox(self.listbox)

        pile = urwid.Pile([])
        loop.create_task(get_info(pile))

        scroll = ScrollBar(Scrollable(pile))
        scroll_frame = urwid.LineBox(scroll)

        columns = urwid.Columns([list_frame, scroll_frame], focus_column=0)
        self.view = urwid.Frame(urwid.AttrWrap( columns, 'body' ))

    def main(self):
        self.loop = urwid.MainLoop(self.view, self.palette,
            event_loop=urwid.AsyncioEventLoop(loop=loop),
            unhandled_input=self.unhandled_input)
        self.loop.run()

    def unhandled_input(self, k):
        if k in ('q','Q'):
            raise urwid.ExitMainLoop()


def get_example_tree():
    tree = {"name":"service","session":[]}
    for i in range(2):
        tree['session'].append({"name":f"session{str(i)}"})
        tree['session'][i]['session']=[]
        for j in range(2):
            tree['session'][i]['session'].append({"name":"connection"+
                                                      str(i) + "." + str(j)})
    return tree

async def get_info(pile):
    while True:
        await asyncio.sleep(0.5)
        t = time.localtime()
        current_time = time.strftime("%H:%M:%S", t)
        text = urwid.Text(f"{current_time}: recv ping-pong")
        pile.contents.append((text, pile.options()))

if __name__ == '__main__':
    sample = get_example_tree()
    Dnetview(sample).main()

